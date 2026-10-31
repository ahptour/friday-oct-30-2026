use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
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
struct Adjacency {
    offsets: Vec<u32>,
    // The high bit selects an exact full-width exception. Ordinary entries
    // retain 21 global node bits and 10 weight bits, independent of ownership.
    edges: Vec<u32>,
    large: Vec<Edge>,
}
impl Adjacency {
    fn build_pair(parts: &[Part], begin: usize, end: usize) -> [Self; 2] {
        let mut pair: [Self; 2] = std::array::from_fn(|_| Self {
            offsets: vec![0; end - begin + 1],
            edges: Vec::new(),
            large: Vec::new(),
        });
        // Each owner repeatedly streams the compact edge list, but its random
        // writes touch only its own small vertex range. Count both directions
        // in one pass; no atomic cursors, per-worker histograms or edge copies.
        for edge in parts.iter().flat_map(Part::iter) {
            for (direction, graph) in pair.iter_mut().enumerate() {
                let node = edge[direction] as usize;
                if (begin..end).contains(&node) {
                    graph.offsets[node - begin + 1] += 1;
                }
            }
        }
        for graph in &mut pair {
            for i in 1..graph.offsets.len() {
                graph.offsets[i] += graph.offsets[i - 1];
            }
            graph.edges = vec![0; *graph.offsets.last().unwrap() as usize];
        }
        for edge in parts.iter().flat_map(Part::iter) {
            for (direction, graph) in pair.iter_mut().enumerate() {
                let node = edge[direction] as usize;
                if !(begin..end).contains(&node) {
                    continue;
                }
                let slot = &mut graph.offsets[node - begin];
                let neighbor = edge[1 - direction];
                graph.edges[*slot as usize] = if neighbor < (1 << 21) && edge[2] < (1 << 10) {
                    neighbor | (edge[2] << 21)
                } else {
                    let index = u32::try_from(graph.large.len()).unwrap();
                    assert!(index < 1 << 31);
                    graph.large.push(Edge {
                        node: neighbor,
                        weight: edge[2],
                    });
                    index | (1 << 31)
                };
                *slot += 1;
            }
        }
        for graph in &mut pair {
            graph.offsets.rotate_right(1);
            graph.offsets[0] = 0;
        }
        pair
    }
    fn neighbors(&self, node: usize) -> impl Iterator<Item = Edge> + '_ {
        self.edges[self.offsets[node] as usize..self.offsets[node + 1] as usize]
            .iter()
            .map(|&packed| {
                if packed >> 31 == 0 {
                    Edge {
                        node: packed & 0x1fffff,
                        weight: packed >> 21,
                    }
                } else {
                    self.large[(packed & 0x7fffffff) as usize]
                }
            })
    }
}
struct Graph {
    shift: u32,
    shards: Vec<Adjacency>,
}
impl Graph {
    fn build(n: usize, parts: &[Part], workers: usize) -> (Self, Self) {
        // Power-of-two ownership makes every query lookup a shift and mask.
        // At full scale twelve workers give seven 262,144-node ranges.
        let span = n.div_ceil(workers).max(1).next_power_of_two();
        let pairs = thread::scope(|scope| {
            let jobs: Vec<_> = (0..n)
                .step_by(span)
                .map(|begin| {
                    scope.spawn(move || Adjacency::build_pair(parts, begin, (begin + span).min(n)))
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().unwrap())
                .collect::<Vec<_>>()
        });
        let mut forward = Vec::with_capacity(pairs.len());
        let mut backward = Vec::with_capacity(pairs.len());
        for [a, b] in pairs {
            forward.push(a);
            backward.push(b);
        }
        let shift = span.trailing_zeros();
        (
            Self {
                shift,
                shards: forward,
            },
            Self {
                shift,
                shards: backward,
            },
        )
    }
    fn shard(&self, node: usize) -> (&Adjacency, usize) {
        (
            &self.shards[node >> self.shift],
            node & ((1usize << self.shift) - 1),
        )
    }
    fn has_edges(&self, node: usize) -> bool {
        let (shard, local) = self.shard(node);
        shard.offsets[local] != shard.offsets[local + 1]
    }
    fn neighbors(&self, node: usize) -> impl Iterator<Item = Edge> + '_ {
        let (shard, local) = self.shard(node);
        shard.neighbors(local)
    }
}
struct Distances {
    // High word is a generation; low word is the exact node ID. Generation
    // zero is vacant, so even u32::MAX would be an ordinary representable key.
    tags: Vec<u64>,
    values: Vec<u64>,
    generation: u32,
    len: usize,
}
impl Distances {
    fn new() -> Self {
        Self {
            tags: vec![0; 1024],
            values: vec![0; 1024],
            generation: 1,
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.len = 0;
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.tags.fill(0);
            self.generation = 1;
        }
    }
    fn slot(&self, node: u32) -> usize {
        let mask = self.tags.len() - 1;
        let key = (u64::from(self.generation) << 32) | u64::from(node);
        let mut slot = (u64::from(node).wrapping_mul(0x9e3779b97f4a7c15) >> 32) as usize & mask;
        while self.tags[slot] != key && self.tags[slot] >> 32 == u64::from(self.generation) {
            slot = (slot + 1) & mask;
        }
        slot
    }
    fn get(&self, node: u32) -> u64 {
        let slot = self.slot(node);
        if self.tags[slot] >> 32 == u64::from(self.generation) {
            self.values[slot]
        } else {
            u64::MAX
        }
    }
    fn improve(&mut self, node: u32, distance: u64) -> bool {
        let mut slot = self.slot(node);
        if self.tags[slot] >> 32 == u64::from(self.generation) {
            if distance >= self.values[slot] {
                return false;
            }
        } else {
            if (self.len + 1) * 4 > self.tags.len() * 3 {
                let capacity = self.tags.len() * 2;
                let old_tags = std::mem::replace(&mut self.tags, vec![0; capacity]);
                let old_values = std::mem::replace(&mut self.values, vec![0; capacity]);
                for (tag, value) in old_tags.into_iter().zip(old_values) {
                    if tag >> 32 == u64::from(self.generation) {
                        let slot = self.slot(tag as u32);
                        self.tags[slot] = tag;
                        self.values[slot] = value;
                    }
                }
                slot = self.slot(node);
            }
            self.len += 1;
            self.tags[slot] = (u64::from(self.generation) << 32) | u64::from(node);
        }
        self.values[slot] = distance;
        true
    }
}

struct Queue {
    buckets: [Vec<(u64, u32)>; 65],
    occupied: u64,
    last: u64,
    len: usize,
}
impl Queue {
    fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| Vec::new()),
            occupied: 0,
            last: 0,
            len: 0,
        }
    }
    fn bucket(&mut self, entry: (u64, u32)) {
        let index = (64 - (entry.0 ^ self.last).leading_zeros()) as usize;
        self.buckets[index].push(entry);
        if index != 0 {
            self.occupied |= 1u64 << (index - 1);
        }
    }
    fn push(&mut self, distance: u64, node: u32) {
        // Nonnegative weights make each direction's extracted distances
        // monotone. Node IDs do not participate in the radix key.
        debug_assert!(distance >= self.last);
        self.bucket((distance, node));
        self.len += 1;
    }
    fn peek(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        if self.buckets[0].is_empty() {
            let index = self.occupied.trailing_zeros() as usize + 1;
            self.last = self.buckets[index]
                .iter()
                .map(|entry| entry.0)
                .min()
                .unwrap();
            self.occupied &= !(1u64 << (index - 1));
            // Every redistributed key moves to a strictly lower bucket;
            // retain the drained bucket's allocation for subsequent queries.
            while let Some(entry) = self.buckets[index].pop() {
                self.bucket(entry);
            }
        }
        Some(self.last)
    }
    fn pop(&mut self) -> Option<(u64, u32)> {
        self.peek()?;
        self.len -= 1;
        self.buckets[0].pop()
    }
    fn len(&self) -> usize {
        self.len
    }
    fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.occupied = 0;
        self.last = 0;
        self.len = 0;
    }
}
struct Search {
    distance: Distances,
    heap: Queue,
}
impl Search {
    fn new() -> Self {
        Self {
            distance: Distances::new(),
            heap: Queue::new(),
        }
    }
    fn start(&mut self, source: u32) {
        self.distance.clear();
        self.heap.clear();
        self.distance.improve(source, 0);
        self.heap.push(0, source);
    }
    fn step(&mut self, other: &Self, graph: &Graph, best: &mut u64) {
        let (distance, node) = self.heap.pop().unwrap();
        if distance != self.distance.get(node) {
            return;
        }
        for Edge { node, weight } in graph.neighbors(node as usize) {
            let candidate = distance + u64::from(weight);
            if self.distance.improve(node, candidate) {
                self.heap.push(candidate, node);
                let back = other.distance.get(node);
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
    if !forward.has_edges(source as usize) || !backward.has_edges(target as usize) {
        return -1;
    }
    let [a, b] = searches;
    a.start(source);
    b.start(target);
    let mut best = u64::MAX;
    while let (Some(left), Some(right)) = (a.heap.peek(), b.heap.peek()) {
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
    let (forward, backward) = Graph::build(n, &parts, workers);
    drop(parts);
    drop(mapping);
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
