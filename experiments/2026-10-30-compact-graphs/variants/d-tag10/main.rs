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
struct Graph {
    offsets: Vec<u32>,
    edges: Vec<u32>,
    large: Vec<Edge>,
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
        let mut edges = vec![0u32; offsets[n] as usize];
        let mut large = Vec::new();
        for edge in parts.iter().flat_map(Part::iter) {
            let slot = &mut offsets[edge[from] as usize];
            let node = edge[1 - from];
            edges[*slot as usize] = if node < (1 << 21) && edge[2] < (1 << 10) {
                node | (edge[2] << 21)
            } else {
                let index = u32::try_from(large.len()).unwrap();
                assert!(index < 1 << 31);
                large.push(Edge {
                    node,
                    weight: edge[2],
                });
                index | (1 << 31)
            };
            *slot += 1;
        }
        // Construction leaves each node's end offset; restore start offsets.
        offsets.rotate_right(1);
        offsets[0] = 0;
        Self {
            offsets,
            edges,
            large,
        }
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
    if forward.offsets[source as usize] == forward.offsets[source as usize + 1]
        || backward.offsets[target as usize] == backward.offsets[target as usize + 1]
    {
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
    let (forward, backward) = thread::scope(|scope| {
        let reverse = scope.spawn(|| Graph::build(n, &parts, true));
        let forward = Graph::build(n, &parts, false);
        (forward, reverse.join().unwrap())
    });
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
