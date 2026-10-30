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
// Narrow positions save one word per adjacency entry; files beyond the narrow
// range use the same algorithm with exact u64 positions, selected once at setup.
trait Position: Copy + Default + Send + Sync {
    fn from_usize(value: usize) -> Self;
    fn as_usize(self) -> usize;
}
impl Position for u32 {
    fn from_usize(value: usize) -> Self {
        u32::try_from(value).expect("narrow mapping checked before construction")
    }
    fn as_usize(self) -> usize {
        self as usize
    }
}
impl Position for u64 {
    fn from_usize(value: usize) -> Self {
        value as u64
    }
    fn as_usize(self) -> usize {
        self as usize
    }
}
fn narrow_positions(size: usize) -> bool {
    u32::try_from(size).is_ok()
}
#[derive(Clone, Copy)]
struct Edge {
    node: u32,
    weight: u32,
}
struct Graph<'a, P> {
    offsets: Vec<u32>,
    positions: Vec<P>,
    input: &'a [u8],
    reverse: bool,
}
impl<'a, P: Position> Graph<'a, P> {
    fn build(n: usize, input: &'a [u8], begin: usize, reverse: bool) -> Self {
        let mut offsets = vec![0u32; n + 1];
        let mut cursor = begin;
        while cursor < input.len() {
            let source = number(input, &mut cursor);
            let target = number(input, &mut cursor);
            number(input, &mut cursor);
            let node = if reverse { target } else { source };
            offsets[node as usize + 1] += 1;
        }
        for i in 1..=n {
            offsets[i] += offsets[i - 1];
        }
        let mut positions = vec![P::default(); offsets[n] as usize];
        cursor = begin;
        while cursor < input.len() {
            let position = P::from_usize(cursor);
            let source = number(input, &mut cursor);
            let target = number(input, &mut cursor);
            number(input, &mut cursor);
            let node = if reverse { target } else { source };
            let slot = &mut offsets[node as usize];
            positions[*slot as usize] = position;
            *slot += 1;
        }
        // Reuse offsets as scatter cursors, then restore adjacency starts.
        offsets.rotate_right(1);
        offsets[0] = 0;
        Self {
            offsets,
            positions,
            input,
            reverse,
        }
    }
    fn neighbors(&self, node: usize) -> impl Iterator<Item = Edge> + '_ {
        self.positions[self.offsets[node] as usize..self.offsets[node + 1] as usize]
            .iter()
            .map(|&position| {
                let mut cursor = position.as_usize();
                let source = number(self.input, &mut cursor);
                let target = number(self.input, &mut cursor);
                let weight = number(self.input, &mut cursor);
                Edge {
                    node: if self.reverse { source } else { target },
                    weight,
                }
            })
    }
    fn is_empty(&self, node: usize) -> bool {
        self.offsets[node] == self.offsets[node + 1]
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
    fn step<P: Position>(&mut self, other: &Self, graph: &Graph<P>, best: &mut u64) {
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
fn shortest<P: Position>(
    forward: &Graph<P>,
    backward: &Graph<P>,
    searches: &mut [Search; 2],
    source: u32,
    target: u32,
) -> i64 {
    if source == target {
        return 0;
    }
    if forward.is_empty(source as usize) || backward.is_empty(target as usize) {
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
    let size = usize::try_from(file.metadata()?.len()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "graph mapping exceeds address space",
        )
    })?;
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
    if narrow_positions(size) {
        run::<u32>(data, out, input, n, start, workers)
    } else {
        run::<u64>(data, out, input, n, start, workers)
    }
}
fn run<P: Position>(
    data: &Path,
    out: &Path,
    input: &[u8],
    n: usize,
    start: usize,
    workers: usize,
) -> io::Result<()> {
    let (forward, backward) = thread::scope(|scope| {
        let reverse = scope.spawn(|| Graph::<P>::build(n, input, start, true));
        let forward = Graph::<P>::build(n, input, start, false);
        (forward, reverse.join().unwrap())
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offset_width_boundaries() {
        assert!(narrow_positions(u32::MAX as usize));
        assert!(!narrow_positions(u32::MAX as usize + 1));
        assert_eq!(
            <u32 as Position>::from_usize(u32::MAX as usize).as_usize(),
            u32::MAX as usize
        );
        assert_eq!(
            <u64 as Position>::from_usize(u32::MAX as usize + 1).as_usize(),
            u32::MAX as usize + 1
        );
    }
    #[test]
    fn both_widths_decode_identically() {
        let input = b"4 5\n0 1 4294967295\n1 2 0\n0 1 3\n2 3 4294967295\n3 0 7\n";
        let mut start = 0;
        number(input, &mut start);
        number(input, &mut start);
        for reverse in [false, true] {
            let small = Graph::<u32>::build(4, input, start, reverse);
            let large = Graph::<u64>::build(4, input, start, reverse);
            for node in 0..4 {
                let a: Vec<_> = small.neighbors(node).map(|e| (e.node, e.weight)).collect();
                let b: Vec<_> = large.neighbors(node).map(|e| (e.node, e.weight)).collect();
                assert_eq!(a, b);
            }
        }
    }
}
