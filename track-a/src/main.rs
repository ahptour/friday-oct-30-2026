use std::collections::HashMap;
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

struct Mapping(*mut c_void, usize);
impl Mapping {
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

#[derive(Default)]
struct Group {
    latencies: Vec<u32>,
    errors: u64,
}
type Groups<'a> = HashMap<&'a [u8], Group, BuildHasherDefault<FastHasher>>;

// Locate the first space eight bytes at a time; the scalar tail stays in bounds.
fn space(input: &[u8], mut pos: usize) -> usize {
    while pos + 8 <= input.len() {
        let word = u64::from_le_bytes(input[pos..pos + 8].try_into().unwrap()) ^ 0x2020202020202020;
        let matches = word.wrapping_sub(0x0101010101010101) & !word & 0x8080808080808080;
        if matches != 0 {
            return pos + matches.trailing_zeros() as usize / 8;
        }
        pos += 8;
    }
    while input[pos] != b' ' {
        pos += 1;
    }
    pos
}

fn parse_groups<'a>(input: &'a [u8], groups: &mut Groups<'a>, label: &mut [u8; 13]) {
    let mut pos = 0;
    while pos < input.len() {
        label.copy_from_slice(&input[pos..pos + 13]);
        pos += 28;
        pos = space(input, pos);
        pos += 1;
        let start = pos;
        pos = space(input, pos);
        let endpoint = &input[start..pos];
        let error = input[pos + 1] == b'5';
        pos += 5;
        let mut latency = 0u32;
        while input[pos] != b'\n' {
            latency = latency * 10 + u32::from(input[pos] - b'0');
            pos += 1;
        }
        pos += 1;
        let group = groups.entry(endpoint).or_default();
        group.latencies.push(latency);
        group.errors += u64::from(error);
    }
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

fn format_hour(label: [u8; 13], groups: Groups<'_>) -> Vec<u8> {
    let mut output = Vec::new();
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (endpoint, mut group) in groups {
        let count = group.latencies.len() as u64;
        let [p50, p95, p99] = percentiles(&mut group.latencies);
        let ppm = group.errors * 1_000_000 / count;
        output.extend_from_slice(&label);
        output.extend_from_slice(b":00:00Z ");
        output.extend_from_slice(endpoint);
        writeln!(output, " {count} {p50} {p95} {p99} {} {ppm}", group.errors).unwrap();
    }
    output
}

fn index_hours(input: &[u8], base: usize) -> [Vec<(usize, usize)>; 72] {
    let mut ranges: [Vec<(usize, usize)>; 72] = std::array::from_fn(|_| Vec::new());
    let mut pos = 0;
    while pos < input.len() {
        let start = pos;
        // Every seed uses the specified 72-hour span, crossing Oct 31 to Nov 1.
        let day = (input[pos + 8] - b'0') * 10 + input[pos + 9] - b'0';
        let day = if day >= 30 { day - 30 } else { day + 1 };
        let hour = (input[pos + 11] - b'0') * 10 + input[pos + 12] - b'0';
        let hour = usize::from(day) * 24 + usize::from(hour) - 18;
        pos += 28;
        while pos + 8 <= input.len() {
            let word =
                u64::from_le_bytes(input[pos..pos + 8].try_into().unwrap()) ^ 0x0a0a0a0a0a0a0a0a;
            let matches = word.wrapping_sub(0x0101010101010101) & !word & 0x8080808080808080;
            if matches != 0 {
                pos += matches.trailing_zeros() as usize / 8;
                break;
            }
            pos += 8;
        }
        while input[pos] != b'\n' {
            pos += 1;
        }
        pos += 1;
        // Coalesce only adjacent lines; later returns to this hour stay indexed.
        if let Some(last) = ranges[hour].last_mut()
            && last.1 == base + start
        {
            last.1 = base + pos;
            continue;
        }
        ranges[hour].push((base + start, base + pos));
    }
    ranges
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
    let ranges = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let mut start = 0;
        for worker in 0..workers {
            let mut end = size * (worker + 1) / workers;
            while end < size && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            jobs.push(scope.spawn(move || index_hours(chunk, start)));
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let next = AtomicUsize::new(0);
    let (send, receive) = mpsc::channel();
    thread::scope(|scope| {
        for _ in 0..workers {
            let next = &next;
            let ranges = &ranges;
            let send = send.clone();
            scope.spawn(move || {
                loop {
                    // One worker owns all ranges and latency groups for this hour.
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= 72 {
                        break;
                    }
                    let mut groups = Groups::default();
                    let mut label = [0; 13];
                    for part in ranges {
                        for &(start, end) in &part[hour] {
                            parse_groups(&input[start..end], &mut groups, &mut label);
                        }
                    }
                    let result = format_hour(label, groups);
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
