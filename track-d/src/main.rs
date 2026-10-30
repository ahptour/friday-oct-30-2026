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
fn parse(input: &[u8]) -> Vec<[u32; 3]> {
    let mut edges = Vec::with_capacity(input.len() / 18);
    let mut cursor = 0;
    while cursor < input.len() {
        edges.push([
            number(input, &mut cursor),
            number(input, &mut cursor),
            number(input, &mut cursor),
        ]);
    }
    edges
}
#[derive(Clone, Copy, Default)]
struct Edge {
    node: u32,
    weight: u32,
}
struct Graph {
    offsets: Vec<u32>,
    edges: Vec<Edge>,
}
impl Graph {
    fn build(n: usize, parts: &[Vec<[u32; 3]>], reverse: bool) -> Self {
        let mut offsets = vec![0u32; n + 1];
        let from = usize::from(reverse);
        for edge in parts.iter().flatten() {
            offsets[edge[from] as usize + 1] += 1;
        }
        for i in 1..=n {
            offsets[i] += offsets[i - 1];
        }
        let mut positions = offsets[..n].to_vec();
        let mut edges = vec![Edge::default(); offsets[n] as usize];
        for edge in parts.iter().flatten() {
            let slot = &mut positions[edge[from] as usize];
            edges[*slot as usize] = Edge {
                node: edge[1 - from],
                weight: edge[2],
            };
            *slot += 1;
        }
        Self { offsets, edges }
    }
    fn neighbors(&self, node: usize) -> &[Edge] {
        &self.edges[self.offsets[node] as usize..self.offsets[node + 1] as usize]
    }
}
struct Search {
    distance: HashMap<u32, u64, BuildHasherDefault<NodeHasher>>,
    heap: BinaryHeap<Reverse<(u64, u32)>>,
}
impl Search {
    fn new() -> Self {
        Self {
            distance: HashMap::default(),
            heap: BinaryHeap::new(),
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
        for &Edge { node, weight } in graph.neighbors(node as usize) {
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
    if forward.neighbors(source as usize).is_empty()
        || backward.neighbors(target as usize).is_empty()
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
