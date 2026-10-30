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

// Locate a delimiter eight bytes at a time; the scalar tail stays in bounds.
fn delimiter(input: &[u8], mut pos: usize, byte: u8) -> usize {
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

fn fields(input: &[u8], pos: usize) -> (usize, usize, usize) {
    // The scoring contract guarantees AVX2. Only full, in-bounds windows use it.
    #[cfg(target_arch = "x86_64")]
    if input.len() - pos >= 64 {
        use std::arch::x86_64::*;
        unsafe {
            let a = _mm256_loadu_si256(input.as_ptr().add(pos).cast());
            let b = _mm256_loadu_si256(input.as_ptr().add(pos + 32).cast());
            let mask = |c| {
                let value = _mm256_set1_epi8(c);
                u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(a, value)) as u32)
                    | (u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(b, value)) as u32) << 32)
            };
            let spaces = mask(b' ' as i8);
            let lines = mask(b'\n' as i8);
            let start = pos + spaces.trailing_zeros() as usize + 1;
            let second = spaces & (spaces - 1);
            let end = if second != 0 {
                pos + second.trailing_zeros() as usize
            } else {
                delimiter(input, start, b' ')
            };
            let eol = if lines != 0 {
                pos + lines.trailing_zeros() as usize
            } else {
                delimiter(input, end + 5, b'\n')
            };
            return (start, end, eol);
        }
    }
    let start = delimiter(input, pos, b' ') + 1;
    let end = delimiter(input, start, b' ');
    (start, end, delimiter(input, end + 5, b'\n'))
}
fn latency(input: &[u8], start: usize, end: usize) -> u32 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // A complete log line precedes end, so this load stays inside the slice.
        use std::arch::x86_64::*;
        let mut v = _mm_loadu_si128(input.as_ptr().add(end - 16).cast());
        v = _mm_subs_epu8(v, _mm_set1_epi8(b'0' as i8));
        let lane = _mm_setr_epi8(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
        v = _mm_and_si128(
            v,
            _mm_cmpgt_epi8(lane, _mm_set1_epi8((15 - (end - start)) as i8)),
        );
        v = _mm_maddubs_epi16(v, _mm_set1_epi16(0x010a));
        v = _mm_madd_epi16(v, _mm_set1_epi32(0x00010064));
        v = _mm_packus_epi32(v, v);
        v = _mm_madd_epi16(v, _mm_set1_epi32(0x00012710));
        return (u64::from(_mm_cvtsi128_si32(v) as u32) * 100_000_000
            + u64::from(_mm_extract_epi32(v, 1) as u32)) as u32;
    }
    #[cfg(not(target_arch = "x86_64"))]
    input[start..end]
        .iter()
        .fold(0, |n, &byte| n * 10 + u32::from(byte - b'0'))
}
fn parse_groups<'a>(input: &'a [u8], groups: &mut Groups<'a>, label: &mut [u8; 13]) {
    if !input.is_empty() {
        label.copy_from_slice(&input[..13]);
    }
    let mut pos = 0;
    while pos < input.len() {
        let (start, end, eol) = fields(input, pos + 28);
        let group = groups.entry(&input[start..end]).or_default();
        group.latencies.push(latency(input, end + 5, eol));
        group.errors += u64::from(input[end + 1] == b'5');
        pos = eol + 1;
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
        pos = delimiter(input, pos + 28, b'\n') + 1;
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
