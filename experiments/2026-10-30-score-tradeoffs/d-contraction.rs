use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};
#[derive(Default)]
struct NodeHasher(u64);
impl Hasher for NodeHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        self.0 = u64::from(u32::from_ne_bytes(bytes.try_into().unwrap()))
            .wrapping_mul(0x9e3779b97f4a7c15);
    }
}
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::thread;

unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
}
struct Mapping(*mut c_void, usize);
impl Mapping {
    fn bytes(&self) -> &[u8] {
        // The read-only mapping remains owned until every scoped worker has joined.
        unsafe { std::slice::from_raw_parts(self.0.cast(), self.1) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            munmap(self.0, self.1);
        }
    }
}
fn number(input: &[u8], cursor: &mut usize) -> u32 {
    let mut value = 0;
    while *cursor < input.len() && input[*cursor].is_ascii_digit() {
        value = value * 10 + u32::from(input[*cursor] - b'0');
        *cursor += 1;
    }
    *cursor += 1;
    value
}
struct Part {
    edges: Vec<u64>,
    large: Vec<[u32; 3]>,
}
impl Part {
    fn iter(&self) -> impl Iterator<Item = [u32; 3]> + '_ {
        self.edges.iter().map(|&v| {
            if v >> 63 != 0 {
                self.large[(v & 0x7fffffffffffffff) as usize]
            } else {
                [
                    (v & 0x1fffff) as u32,
                    ((v >> 21) & 0x1fffff) as u32,
                    (v >> 42) as u32,
                ]
            }
        })
    }
}
fn parse(input: &[u8]) -> Part {
    let mut part = Part {
        edges: Vec::with_capacity(input.len() / 18),
        large: Vec::new(),
    };
    let mut cursor = 0;
    while cursor < input.len() {
        let edge = [
            number(input, &mut cursor),
            number(input, &mut cursor),
            number(input, &mut cursor),
        ];
        // Three 21-bit fields fit below the tag bit; larger values stay exact.
        let packed = if edge.iter().all(|&value| value < (1 << 21)) {
            u64::from(edge[0]) | (u64::from(edge[1]) << 21) | (u64::from(edge[2]) << 42)
        } else {
            let index = part.large.len() as u64;
            part.large.push(edge);
            (1 << 63) | index
        };
        part.edges.push(packed);
    }
    part
}
#[derive(Clone, Copy, Default)]
struct Edge {
    node: u32,
    weight: u32,
}
struct Graph {
    offsets: Vec<u32>,
    edges: Vec<Edge>,
    wide: HashMap<u32, u64>,
}
impl Graph {
    fn build(n: usize, parts: &[Part], reverse: bool) -> Self {
        let mut offsets = vec![0u32; n + 1];
        let from = usize::from(reverse);
        for edge in parts.iter().flat_map(Part::iter) {
            offsets[edge[from] as usize + 1] += 1;
        }
        for i in 1..=n {
            offsets[i] += offsets[i - 1];
        }
        let mut edges = vec![Edge::default(); offsets[n] as usize];
        for edge in parts.iter().flat_map(Part::iter) {
            let slot = &mut offsets[edge[from] as usize];
            edges[*slot as usize] = Edge {
                node: edge[1 - from],
                weight: edge[2],
            };
            *slot += 1;
        }
        // Construction leaves each node's end offset; restore start offsets.
        offsets.rotate_right(1);
        offsets[0] = 0;
        Self { offsets, edges, wide: HashMap::new() }
    }
    fn weight(&self, index: usize) -> u64 {
        let weight = self.edges[index].weight;
        if weight == u32::MAX {
            self.wide.get(&(index as u32)).copied().unwrap_or(u64::from(weight))
        } else {
            u64::from(weight)
        }
    }
    fn indices(&self, node: usize) -> std::ops::Range<usize> {
        self.offsets[node] as usize..self.offsets[node + 1] as usize
    }
    fn normalize(&mut self) {
        for node in 0..self.offsets.len() - 1 {
            let range = self.indices(node);
            let row = &mut self.edges[range];
            row.sort_unstable_by_key(|edge| (edge.node, edge.weight));
            let mut previous = u32::MAX;
            for edge in row {
                let next = edge.node;
                if next == node as u32 || next == previous {
                    edge.node = u32::MAX;
                }
                previous = next;
            }
        }
    }
    // Degree two guarantees the arc to the eliminated vertex supplies a free
    // slot for its sole possible replacement. No per-vertex allocation is needed.
    fn replace(&mut self, node: u32, removed: u32, shortcut: Option<(u32, u64)>) {
        let mut old = None;
        let mut existing = None;
        for i in self.indices(node as usize) {
            if self.edges[i].node == removed { old = Some(i); }
            if shortcut.is_some_and(|(target, _)| self.edges[i].node == target) {
                existing = Some(i);
            }
        }
        let old = old.unwrap();
        self.edges[old].node = u32::MAX;
        self.wide.remove(&(old as u32));
        if let Some((target, weight)) = shortcut {
            let index = existing.unwrap_or(old);
            if existing.is_none() || weight < self.weight(index) {
                self.edges[index] = Edge { node: target, weight: weight.min(u64::from(u32::MAX)) as u32 };
                if weight > u64::from(u32::MAX) { self.wide.insert(index as u32, weight); }
                else { self.wide.remove(&(index as u32)); }
            }
        }
    }
    fn compact(&mut self) {
        let mut write = 0;
        let mut wide = HashMap::new();
        for node in 0..self.offsets.len() - 1 {
            let range = self.indices(node);
            self.offsets[node] = write as u32;
            for i in range {
                if self.edges[i].node != u32::MAX {
                    let weight = self.weight(i);
                    self.edges[write] = self.edges[i];
                    if weight > u64::from(u32::MAX) { wide.insert(write as u32, weight); }
                    write += 1;
                }
            }
        }
        *self.offsets.last_mut().unwrap() = write as u32;
        self.edges.truncate(write);
        self.edges.shrink_to_fit();
        self.wide = wide;
    }
    fn neighbors(&self, node: usize) -> &[Edge] {
        &self.edges[self.offsets[node] as usize..self.offsets[node + 1] as usize]
    }
}
// Every shortcut represents a directed walk of equal weight; every path
// through an eliminated vertex has that shortcut. Nonnegative cycles can be
// removed, so preserving all query endpoints preserves their exact distances.
fn contract(forward: &mut Graph, backward: &mut Graph, queries: &[(u32, u32)]) {
    forward.normalize();
    backward.normalize();
    let n = forward.offsets.len() - 1;
    let mut state = vec![0u8; n]; // protected, queued, removed
    for &(source, target) in queries { state[source as usize] = 1; state[target as usize] = 1; }
    fn eligible(node: usize, a: &Graph, b: &Graph) -> bool {
        let mut neighbors = [u32::MAX; 2];
        for graph in [a, b] {
            for edge in graph.neighbors(node) {
                if edge.node == u32::MAX || neighbors.contains(&edge.node) { continue; }
                if let Some(slot) = neighbors.iter_mut().find(|slot| **slot == u32::MAX) { *slot = edge.node; }
                else { return false; }
            }
        }
        true
    }
    let mut queue = VecDeque::new();
    for (node, flags) in state.iter_mut().enumerate() {
        if *flags == 0 && eligible(node, forward, backward) { *flags = 2; queue.push_back(node as u32); }
    }
    let mut removed = 0;
    while let Some(node) = queue.pop_front() {
        state[node as usize] &= !2;
        if state[node as usize] != 0 || !eligible(node as usize, forward, backward) { continue; }
        let mut incoming = [(u32::MAX, 0u64); 2];
        let mut outgoing = incoming;
        for (graph, arcs) in [(&*backward, &mut incoming), (&*forward, &mut outgoing)] {
            let mut count = 0;
            for i in graph.indices(node as usize) {
                if graph.edges[i].node != u32::MAX {
                    arcs[count] = (graph.edges[i].node, graph.weight(i)); count += 1;
                }
            }
        }
        for (graph, arcs, others) in [(&mut *forward, &incoming, &outgoing), (&mut *backward, &outgoing, &incoming)] {
            for &(neighbor, weight) in arcs {
                if neighbor == u32::MAX { continue; }
                let shortcut = others.iter().find(|&&(target, _)| target != u32::MAX && target != neighbor)
                    .map(|&(target, other)| (target, weight.saturating_add(other)));
                graph.replace(neighbor, node, shortcut);
            }
            for i in graph.indices(node as usize) { graph.edges[i].node = u32::MAX; graph.wide.remove(&(i as u32)); }
        }
        state[node as usize] = 4;
        removed += 1;
        for &(neighbor, _) in incoming.iter().chain(&outgoing) {
            if neighbor != u32::MAX && state[neighbor as usize] == 0 && eligible(neighbor as usize, forward, backward) {
                state[neighbor as usize] = 2; queue.push_back(neighbor);
            }
        }
    }
    if std::env::var_os("D_DIAGNOSTIC").is_some() { eprintln!("removed={removed} nodes={n}"); }
    forward.compact();
    backward.compact();
}
#[derive(Default)]
struct Queue {
    small: BinaryHeap<Reverse<u64>>,
    large: BinaryHeap<Reverse<(u64, u32)>>,
}
impl Queue {
    fn new() -> Self {
        Self::default()
    }
    fn push(&mut self, Reverse((distance, node)): Reverse<(u64, u32)>) {
        if distance <= u64::from(u32::MAX) {
            self.small.push(Reverse((distance << 32) | u64::from(node)));
        } else {
            self.large.push(Reverse((distance, node)));
        }
    }
    fn peek(&self) -> Option<Reverse<(u64, u32)>> {
        self.small
            .peek()
            .map(|&Reverse(value)| Reverse((value >> 32, value as u32)))
            .or_else(|| self.large.peek().copied())
    }
    fn pop(&mut self) -> Option<Reverse<(u64, u32)>> {
        self.small
            .pop()
            .map(|Reverse(value)| Reverse((value >> 32, value as u32)))
            .or_else(|| self.large.pop())
    }
    fn len(&self) -> usize {
        self.small.len() + self.large.len()
    }
    fn clear(&mut self) {
        self.small.clear();
        self.large.clear();
    }
}
struct Search {
    distance: HashMap<u32, u64, BuildHasherDefault<NodeHasher>>,
    heap: Queue,
}
impl Search {
    fn new() -> Self {
        Self {
            distance: HashMap::default(),
            heap: Queue::new(),
        }
    }
    fn start(&mut self, source: u32) {
        self.distance.clear();
        self.heap.clear();
        self.distance.insert(source, 0);
        self.heap.push(Reverse((0, source)));
    }
    fn step(&mut self, other: &Self, graph: &Graph, best: &mut u64) {
        let Reverse((distance, node)) = self.heap.pop().unwrap();
        if distance != self.distance[&node] {
            return;
        }
        for index in graph.indices(node as usize) {
            let node = graph.edges[index].node;
            let candidate = distance.saturating_add(graph.weight(index));
            let slot = self.distance.entry(node).or_insert(u64::MAX);
            if candidate < *slot {
                *slot = candidate;
                self.heap.push(Reverse((candidate, node)));
                let back = other.distance.get(&node).copied().unwrap_or(u64::MAX);
                if back != u64::MAX {
                    *best = (*best).min(candidate.saturating_add(back));
                }
            }
        }
    }
}
fn shortest(
    forward: &Graph,
    backward: &Graph,
    searches: &mut [Search; 2],
    source: u32,
    target: u32,
) -> u64 {
    if source == target {
        return 0;
    }
    if forward.neighbors(source as usize).is_empty()
        || backward.neighbors(target as usize).is_empty()
    {
        return u64::MAX;
    }
    let [a, b] = searches;
    a.start(source);
    b.start(target);
    let mut best = u64::MAX;
    while let (Some(Reverse((left, _))), Some(Reverse((right, _)))) = (a.heap.peek(), b.heap.peek())
    {
        // The sum of frontier minima is a lower bound for any undiscovered path.
        if left.saturating_add(right) >= best {
            break;
        }
        // Balancing frontier sizes avoids exploring a large cheap-weight region
        // while the other frontier waits behind a high-weight first edge.
        if a.heap.len() <= b.heap.len() {
            a.step(b, forward, &mut best);
        } else {
            b.step(a, backward, &mut best);
        }
    }
    best
}
fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let begin_time = std::time::Instant::now();
    let input_queries = fs::read(data.join("queries.txt"))?;
    let mut cursor = 0;
    let mut queries = Vec::new();
    while cursor < input_queries.len() {
        queries.push((number(&input_queries, &mut cursor), number(&input_queries, &mut cursor)));
    }
    let file = File::open(data.join("graph.txt"))?;
    let size = file.metadata()?.len() as usize;
    let ptr = unsafe { mmap(std::ptr::null_mut(), size, 1, 2, file.as_raw_fd(), 0) };
    if ptr as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let mapping = Mapping(ptr, size);
    let input = mapping.bytes();
    let mut start = 0;
    let n = number(input, &mut start) as usize;
    let _m = number(input, &mut start);
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12);
    let parts = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let begin = start;
        for worker in 0..workers {
            let mut end = begin + (size - begin) * (worker + 1) / workers;
            while end < size && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            jobs.push(scope.spawn(move || parse(chunk)));
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let parsed_time = std::time::Instant::now();
    let (mut forward, mut backward) = thread::scope(|scope| {
        let reverse = scope.spawn(|| Graph::build(n, &parts, true));
        let forward = Graph::build(n, &parts, false);
        (forward, reverse.join().unwrap())
    });
    drop(parts);
    drop(mapping);
    let built_time = std::time::Instant::now();
    contract(&mut forward, &mut backward, &queries);
    let contracted_time = std::time::Instant::now();
    let answers: Vec<_> = queries.iter().map(|_| AtomicU64::new(u64::MAX)).collect();
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        for _ in 0..workers.min(queries.len()) {
            scope.spawn(|| {
                let mut searches = [Search::new(), Search::new()];
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(source, target)) = queries.get(i) else {
                        break;
                    };
                    answers[i].store(
                        shortest(&forward, &backward, &mut searches, source, target),
                        Ordering::Relaxed,
                    );
                }
            });
        }
    });
    if std::env::var_os("D_DIAGNOSTIC").is_some() {
        eprintln!("parse={:?} build={:?} contract_compact={:?} query={:?} edges={}", parsed_time-begin_time, built_time-parsed_time, contracted_time-built_time, contracted_time.elapsed(), forward.edges.len());
    }
    fs::create_dir_all(out)?;
    let mut output = Vec::new();
    for answer in answers {
        let answer = answer.into_inner();
        if answer == u64::MAX { writeln!(output, "-1").unwrap(); }
        else { writeln!(output, "{answer}").unwrap(); }
    }
    fs::write(out.join("distances.txt"), output)
}
fn main() -> io::Result<()> {
    let mut data = PathBuf::from("/data");
    let mut out = PathBuf::from("/out");
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data") => data = args.next().map(PathBuf::from).ok_or_else(usage)?,
            Some("--out") => out = args.next().map(PathBuf::from).ok_or_else(usage)?,
            _ => return Err(usage()),
        }
    }
    solve(&data, &out)
}
fn usage() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: solution [--data DIRECTORY] [--out DIRECTORY]",
    )
}
