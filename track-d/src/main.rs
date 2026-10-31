use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
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
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicUsize, Ordering};
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
const INLINE: usize = 4;
const WIDE: u32 = 1 << 31;
#[derive(Clone, Copy)]
struct Edge {
    node: u32,
    weight: u32,
}
// One 20-byte record per vertex, per direction. No degree-sized allocation or
// edge-sized temporary is needed for the common case.
struct Node {
    degree: AtomicU32,
    edges: [AtomicU32; INLINE],
}
struct Graph {
    nodes: Vec<Node>,
    overflow: HashMap<u32, Vec<Edge>, BuildHasherDefault<NodeHasher>>,
}
impl Graph {
    fn new(n: usize) -> Self {
        Self {
            nodes: (0..n)
                .map(|_| Node {
                    degree: AtomicU32::new(0),
                    edges: std::array::from_fn(|_| AtomicU32::new(WIDE)),
                })
                .collect(),
            overflow: HashMap::default(),
        }
    }
    fn insert(&self, from: u32, to: u32, weight: u32, overflow: &mut Vec<[u32; 3]>) {
        let node = &self.nodes[from as usize];
        // Reservation only: no reader examines the graph until all producers
        // join. Every reserved slot has one writer; relaxed atomics suffice.
        let slot = node.degree.fetch_add(1, Ordering::Relaxed) as usize;
        if slot < INLINE && to < (1 << 21) && weight < (1 << 10) {
            node.edges[slot].store(to | (weight << 21), Ordering::Relaxed);
        } else {
            // A wide edge leaves its reserved inline slot tagged. High-degree
            // and full-width edges are kept exactly, with no capacity bound.
            overflow.push([from, to, weight]);
        }
    }
    fn finish(&mut self, parts: Vec<Vec<[u32; 3]>>) {
        for part in parts {
            for [from, node, weight] in part {
                self.overflow
                    .entry(from)
                    .or_default()
                    .push(Edge { node, weight });
            }
        }
    }
    fn empty(&self, node: u32) -> bool {
        self.nodes[node as usize].degree.load(Ordering::Relaxed) == 0
    }
    fn neighbors(&self, node: usize) -> impl Iterator<Item = Edge> + '_ {
        let head = &self.nodes[node];
        let degree = head.degree.load(Ordering::Relaxed) as usize;
        let count = degree.min(INLINE);
        let mut packed = [WIDE; INLINE];
        let mut overflow = degree > INLINE;
        for (dst, src) in packed.iter_mut().zip(&head.edges).take(count) {
            *dst = src.load(Ordering::Relaxed);
            overflow |= *dst & WIDE != 0;
        }
        let extra = if overflow {
            self.overflow
                .get(&(node as u32))
                .map(Vec::as_slice)
                .unwrap_or(&[])
        } else {
            &[]
        };
        packed
            .into_iter()
            .take(count)
            .filter(|packed| packed & WIDE == 0)
            .map(|packed| Edge {
                node: packed & 0x1fffff,
                weight: packed >> 21,
            })
            .chain(extra.iter().copied())
    }
}
fn parse(input: &[u8], graphs: &[Graph; 2]) -> [Vec<[u32; 3]>; 2] {
    let mut overflow = [Vec::new(), Vec::new()];
    let mut cursor = 0;
    while cursor < input.len() {
        let from = number(input, &mut cursor);
        let to = number(input, &mut cursor);
        let weight = number(input, &mut cursor);
        graphs[0].insert(from, to, weight, &mut overflow[0]);
        graphs[1].insert(to, from, weight, &mut overflow[1]);
    }
    overflow
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
        for Edge { node, weight } in graph.neighbors(node as usize) {
            let candidate = distance + u64::from(weight);
            let slot = self.distance.entry(node).or_insert(u64::MAX);
            if candidate < *slot {
                *slot = candidate;
                self.heap.push(Reverse((candidate, node)));
                let back = other.distance.get(&node).copied().unwrap_or(u64::MAX);
                if back != u64::MAX {
                    *best = (*best).min(candidate + back);
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
) -> i64 {
    if source == target {
        return 0;
    }
    if forward.empty(source) || backward.empty(target) {
        return -1;
    }
    let [a, b] = searches;
    a.start(source);
    b.start(target);
    let mut best = u64::MAX;
    while let (Some(Reverse((left, _))), Some(Reverse((right, _)))) = (a.heap.peek(), b.heap.peek())
    {
        // The sum of frontier minima is a lower bound for any undiscovered path.
        if left + right >= best {
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
    if best == u64::MAX { -1 } else { best as i64 }
}
fn solve(data: &Path, out: &Path) -> io::Result<()> {
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
    // The arrays are the final graph. Producers reserve disjoint slots directly
    // while parsing; only overflow records survive as temporary edge storage.
    let mut graphs = [Graph::new(n), Graph::new(n)];
    let parsed = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let begin = start;
        for worker in 0..workers {
            let mut end = begin + (size - begin) * (worker + 1) / workers;
            while end < size && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            let graphs = &graphs;
            jobs.push(scope.spawn(move || parse(chunk, graphs)));
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    drop(mapping);
    let mut overflow: [Vec<Vec<[u32; 3]>>; 2] = std::array::from_fn(|_| Vec::new());
    for [forward, backward] in parsed {
        overflow[0].push(forward);
        overflow[1].push(backward);
    }
    // Merge only exceptions after producer joins. Query workers see immutable
    // graph state, including every wide, duplicate, parallel and self-loop edge.
    let [forward_overflow, backward_overflow] = overflow;
    graphs[0].finish(forward_overflow);
    graphs[1].finish(backward_overflow);
    let [forward, backward] = graphs;
    let input = fs::read(data.join("queries.txt"))?;
    let mut cursor = 0;
    let mut queries = Vec::new();
    while cursor < input.len() {
        queries.push((number(&input, &mut cursor), number(&input, &mut cursor)));
    }
    let answers: Vec<_> = queries.iter().map(|_| AtomicI64::new(-1)).collect();
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
    fs::create_dir_all(out)?;
    let mut output = Vec::new();
    for answer in answers {
        writeln!(output, "{}", answer.into_inner()).unwrap();
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
