use std::collections::HashMap;
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
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
    fn madvise(addr: *mut c_void, len: usize, advice: i32) -> i32;
}

struct Mapping(*mut c_void, usize);
// Input mappings are read-only; arena mutation has exclusive ownership during
// parsing. Later readers consume disjoint hour blocks before reclamation.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}
impl Mapping {
    fn allocate(len: usize) -> io::Result<Self> {
        #[cfg(target_os = "linux")]
        let flags = 0x22; // MAP_PRIVATE | MAP_ANONYMOUS
        #[cfg(not(target_os = "linux"))]
        let flags = 0x1002;
        let ptr = unsafe { mmap(std::ptr::null_mut(), len, 3, flags, -1, 0) };
        if ptr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        #[cfg(target_os = "linux")]
        unsafe {
            madvise(ptr, len, 15);
        } // Keep partially used arenas off huge pages.
        Ok(Self(ptr, len))
    }
    fn bytes(&self) -> &[u8] {
        // The successful mapping is read-only, and this borrow cannot outlive it.
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

#[derive(Default)]
struct FastHasher(u64);
impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() >= 8 {
            let word = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let value = word(0).wrapping_mul(0x517cc1b727220a95)
                ^ word((bytes.len() - 8) / 2).rotate_left(21)
                ^ word(bytes.len() - 8).rotate_left(43);
            self.0 ^= value.wrapping_mul(0x9e3779b97f4a7c15);
            self.0 ^= self.0 >> 32;
        } else {
            for &byte in bytes {
                self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x517cc1b727220a95);
            }
        }
    }
}

const BLOCK_BYTES: usize = 16 * 1024;
const CAPACITY: usize = (BLOCK_BYTES - 16) / 4;
const NONE: u32 = u32::MAX;
#[repr(C)]
struct Block {
    previous: u32,
    count: u32,
    hour: u32,
    reserved: u32,
    records: [u32; CAPACITY],
}
struct Part<'a> {
    arena: Mapping,
    used: usize,
    last: [u32; 72],
    wide: [Vec<(u32, u32)>; 72],
    labels: [[u8; 13]; 72],
    names: Vec<&'a [u8]>,
    rank: Vec<usize>,
}
impl Part<'_> {
    fn push(&mut self, hour: usize, id: u32, error: u32, latency: u32) {
        let key = id.checked_mul(2).unwrap() | error;
        if id >= 2048 || latency >= 1 << 20 {
            self.wide[hour].push((latency, key));
            return;
        }
        let blocks = self.arena.0.cast::<Block>();
        let mut index = self.last[hour];
        if index == NONE || unsafe { (*blocks.add(index as usize)).count as usize == CAPACITY } {
            assert!(self.used < self.arena.1 / BLOCK_BYTES);
            index = u32::try_from(self.used).unwrap();
            let block = unsafe { &mut *blocks.add(self.used) };
            block.previous = self.last[hour];
            block.hour = hour as u32;
            block.count = 0;
            self.last[hour] = index;
            self.used += 1;
        }
        let block = unsafe { &mut *blocks.add(index as usize) };
        block.records[block.count as usize] = (latency << 12) | key;
        block.count += 1;
    }
    fn each(&self, hour: usize, mut visit: impl FnMut(usize, u32, u32)) {
        let mut index = self.last[hour];
        while index != NONE {
            // Only this hour's worker reads this block, before marking it done.
            let block = unsafe { &*self.arena.0.cast::<Block>().add(index as usize) };
            for &word in &block.records[..block.count as usize] {
                visit(
                    self.rank[((word >> 1) & 2047) as usize],
                    word & 1,
                    word >> 12,
                );
            }
            index = block.previous;
        }
        for &(latency, key) in &self.wide[hour] {
            visit(self.rank[(key >> 1) as usize], key & 1, latency);
        }
    }
    fn release(&self, kept: &mut usize, finished: &[AtomicBool; 72]) {
        let mut end = *kept;
        while end < self.used {
            let block = unsafe { &*self.arena.0.cast::<Block>().add(end) };
            if !finished[block.hour as usize].load(Ordering::Acquire) {
                break;
            }
            end += 1;
        }
        if end - *kept >= 16 || (end == self.used && end > *kept) {
            // Whole aligned blocks only; Acquire observes all readers finishing.
            unsafe {
                madvise(
                    self.arena.0.cast::<u8>().add(*kept * BLOCK_BYTES).cast(),
                    (end - *kept) * BLOCK_BYTES,
                    4,
                );
            }
            *kept = end;
        }
    }
}
fn find(input: &[u8], mut pos: usize, byte: u8) -> usize {
    let repeated = u64::from(byte) * 0x0101010101010101;
    while pos + 8 <= input.len() {
        let word = u64::from_le_bytes(input[pos..pos + 8].try_into().unwrap()) ^ repeated;
        let matches = word.wrapping_sub(0x0101010101010101) & !word & 0x8080808080808080;
        if matches != 0 {
            return pos + matches.trailing_zeros() as usize / 8;
        }
        pos += 8;
    }
    while input[pos] != byte {
        pos += 1;
    }
    pos
}
type Scan = unsafe fn(*const u8) -> (u64, u64);
unsafe fn scalar_masks(ptr: *const u8) -> (u64, u64) {
    let (mut spaces, mut lines) = (0, 0);
    for i in 0..64 {
        let byte = unsafe { *ptr.add(i) };
        spaces |= u64::from(byte == b' ') << i;
        lines |= u64::from(byte == b'\n') << i;
    }
    (spaces, lines)
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn vector_masks(ptr: *const u8) -> (u64, u64) {
    use std::arch::x86_64::*;
    let a = unsafe { _mm256_loadu_si256(ptr.cast()) };
    let b = unsafe { _mm256_loadu_si256(ptr.add(32).cast()) };
    let mask = |byte| {
        let value = _mm256_set1_epi8(byte);
        u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(a, value)) as u32)
            | (u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(b, value)) as u32) << 32)
    };
    (mask(b' ' as i8), mask(b'\n' as i8))
}
fn classifier() -> Scan {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") {
        return vector_masks;
    }
    scalar_masks
}
fn latency(input: &[u8]) -> u32 {
    let mut value = 0u64;
    let mut chunks = input.chunks_exact(8);
    for chunk in &mut chunks {
        let v = u64::from_le_bytes(chunk.try_into().unwrap()) - 0x3030303030303030;
        let v = (v & 0x00ff00ff00ff00ff) * 10 + ((v >> 8) & 0x00ff00ff00ff00ff);
        let v = (v & 0x0000ffff0000ffff) * 100 + ((v >> 16) & 0x0000ffff0000ffff);
        value = value * 100_000_000 + (v & 0xffffffff) * 10_000 + (v >> 32);
    }
    for &byte in chunks.remainder() {
        value = value * 10 + u64::from(byte - b'0');
    }
    value as u32
}
fn parse(input: &[u8]) -> io::Result<Part<'_>> {
    // The shortest legal line has 40 bytes; allow one partial block per hour.
    let bytes = (input.len() / 40 / CAPACITY + 73) * BLOCK_BYTES;
    let mut part = Part {
        arena: Mapping::allocate(bytes)?,
        used: 0,
        last: [NONE; 72],
        wide: std::array::from_fn(|_| Vec::new()),
        labels: [[0; 13]; 72],
        names: Vec::new(),
        rank: Vec::new(),
    };
    let mut ids: HashMap<&[u8], u32, BuildHasherDefault<FastHasher>> = HashMap::default();
    let scan = classifier();
    let mut pos = 0;
    while pos < input.len() {
        let day = (input[pos + 8] - b'0') * 10 + input[pos + 9] - b'0';
        let day = if day >= 30 { day - 30 } else { day + 1 };
        let hour = usize::from(day) * 24
            + usize::from((input[pos + 11] - b'0') * 10 + input[pos + 12] - b'0')
            - 18;
        if part.labels[hour][0] == 0 {
            part.labels[hour].copy_from_slice(&input[pos..pos + 13]);
        }
        let m = pos + 28;
        let remaining = &input[m..];
        let mut tail;
        let ptr = if remaining.len() >= 64 {
            remaining.as_ptr()
        } else {
            tail = [0u8; 64];
            tail[..remaining.len()].copy_from_slice(remaining);
            tail.as_ptr()
        };
        let (spaces, lines) = unsafe { scan(ptr) };
        let start = m + spaces.trailing_zeros() as usize + 1;
        let second = spaces & (spaces - 1);
        let end = if second != 0 {
            m + second.trailing_zeros() as usize
        } else {
            find(input, start, b' ')
        };
        let eol = if lines != 0 {
            m + lines.trailing_zeros() as usize
        } else {
            find(input, end + 5, b'\n')
        };
        let name = &input[start..end];
        let next_id = u32::try_from(part.names.len()).unwrap();
        let id = *ids.entry(name).or_insert_with(|| {
            part.names.push(name);
            next_id
        });
        part.push(
            hour,
            id,
            u32::from(input[end + 1] == b'5'),
            latency(&input[end + 5..eol]),
        );
        pos = eol + 1;
    }
    Ok(part)
}

fn percentiles(values: &mut [u32]) -> [u32; 3] {
    let count = values.len();
    let mut result = [0; 3];
    let mut offset = 0;
    let mut remaining = values;
    for (slot, percentile) in result.iter_mut().zip([50, 95, 99]) {
        let rank = (percentile * count).div_ceil(100) - 1;
        remaining.select_nth_unstable(rank - offset);
        // Retain the pivot for small groups with coinciding percentile ranks.
        remaining = &mut remaining[rank - offset..];
        *slot = remaining[0];
        offset = rank;
    }
    result
}

fn format_hour(
    hour: usize,
    parts: &[Part<'_>],
    names: &[&[u8]],
    starts: &mut [usize],
    errors: &mut [u64],
    values: &mut Vec<u32>,
) -> Vec<u8> {
    starts.fill(0);
    errors.fill(0);
    for part in parts {
        part.each(hour, |rank, error, _| {
            starts[rank + 1] += 1;
            errors[rank] += u64::from(error);
        });
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }
    values.resize(*starts.last().unwrap(), 0);
    let mut cursor = starts[..names.len()].to_vec();
    for part in parts {
        part.each(hour, |rank, _, latency| {
            values[cursor[rank]] = latency;
            cursor[rank] += 1;
        });
    }
    let label = parts
        .iter()
        .find(|part| part.labels[hour][0] != 0)
        .map(|part| part.labels[hour]);
    let mut output = Vec::new();
    for (rank, &name) in names.iter().enumerate() {
        let count = starts[rank + 1] - starts[rank];
        if count == 0 {
            continue;
        }
        let [p50, p95, p99] = percentiles(&mut values[starts[rank]..starts[rank + 1]]);
        let errors = errors[rank];
        let ppm = errors * 1_000_000 / count as u64;
        output.extend_from_slice(&label.unwrap());
        output.extend_from_slice(b":00:00Z ");
        output.extend_from_slice(name);
        writeln!(output, " {count} {p50} {p95} {p99} {errors} {ppm}").unwrap();
    }
    output
}

fn summarize(data: &Path, out: &Path) -> io::Result<()> {
    let file = File::open(data.join("requests.log"))?;
    let size = file.metadata()?.len() as usize;
    fs::create_dir_all(out)?;
    let mut output = File::create(out.join("summary.txt"))?;
    if size == 0 {
        return Ok(());
    }
    // PROT_READ | MAP_PRIVATE on Linux and macOS; no bytes are modified.
    let ptr = unsafe { mmap(std::ptr::null_mut(), size, 1, 2, file.as_raw_fd(), 0) };
    if ptr as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let mapping = Mapping(ptr, size);
    let input = mapping.bytes();
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(size.div_ceil(64 * 1024));
    let mut parts = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let mut start = 0;
        for worker in 0..workers {
            let mut end = size * (worker + 1) / workers;
            while end < size && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            jobs.push(scope.spawn(move || parse(chunk)));
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<io::Result<Vec<_>>>()
    })?;
    let mut names: Vec<_> = parts
        .iter()
        .flat_map(|part| part.names.iter().copied())
        .collect();
    names.sort_unstable();
    names.dedup();
    for part in &mut parts {
        part.rank = part
            .names
            .iter()
            .map(|name| names.binary_search(name).unwrap())
            .collect();
    }
    let next = AtomicUsize::new(0);
    let finished: [AtomicBool; 72] = std::array::from_fn(|_| AtomicBool::new(false));
    let kept = Mutex::new(vec![0; parts.len()]);
    let (send, receive) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..workers {
            let (next, parts, names, finished, kept, send) =
                (&next, &parts, &names, &finished, &kept, send.clone());
            scope.spawn(move || {
                let mut starts = vec![0; names.len() + 1];
                let mut errors = vec![0; names.len()];
                let mut values = Vec::new();
                loop {
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= 72 {
                        break;
                    }
                    let result =
                        format_hour(hour, parts, names, &mut starts, &mut errors, &mut values);
                    finished[hour].store(true, Ordering::Release);
                    if let Ok(mut fronts) = kept.try_lock() {
                        for (part, front) in parts.iter().zip(fronts.iter_mut()) {
                            part.release(front, finished);
                        }
                    }
                    if send.send((hour, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(send);
        let mut pending: Vec<Option<Vec<u8>>> = (0..72).map(|_| None).collect();
        let mut written = 0;
        for (hour, result) in receive {
            pending[hour] = Some(result);
            while written < 72 {
                let Some(bytes) = pending[written].take() else {
                    break;
                };
                output.write_all(&bytes)?;
                written += 1;
            }
        }
        Ok(())
    })
}

fn main() -> io::Result<()> {
    let mut data = PathBuf::from("/data");
    let mut out = PathBuf::from("/out");
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data") => {
                data = args.next().map(PathBuf::from).ok_or_else(usage)?;
            }
            Some("--out") => {
                out = args.next().map(PathBuf::from).ok_or_else(usage)?;
            }
            _ => return Err(usage()),
        }
    }
    summarize(&data, &out)
}
fn usage() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: solution [--data DIRECTORY] [--out DIRECTORY]",
    )
}
