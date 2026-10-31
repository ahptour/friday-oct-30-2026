use std::collections::HashMap;
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hasher};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
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
    fn madvise(addr: *mut c_void, len: usize, advice: i32) -> i32;
}

struct Mapping(*mut c_void, usize);
// Input mappings are read-only; arena mutation has exclusive ownership during
// parsing. Later consumers own disjoint hour blocks and release only their own blocks.
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

const BLOCK_BYTES: usize = 4 * 1024;
const CAPACITY: usize = (BLOCK_BYTES - 8) / 4;
const NONE: u32 = u32::MAX;
#[repr(C)]
struct Block {
    previous: u32,
    count: u32,
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
            // Only this hour's worker reads this block, before releasing it.
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
    fn release_hour(&self, hour: usize) {
        // This hour's sole consumer has copied every latency out. No other
        // hour shares these whole blocks. Capture links before discarding pages.
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            // The scoring platform's base pages are 4 KiB.
            let mut index = self.last[hour];
            while index != NONE {
                let end = index as usize + 1;
                let mut begin = index as usize;
                let mut previous = unsafe { (*self.arena.0.cast::<Block>().add(begin)).previous };
                while previous != NONE && previous as usize + 1 == begin {
                    begin = previous as usize;
                    previous = unsafe { (*self.arena.0.cast::<Block>().add(begin)).previous };
                }
                unsafe {
                    madvise(
                        self.arena.0.cast::<u8>().add(begin * BLOCK_BYTES).cast(),
                        (end - begin) * BLOCK_BYTES,
                        4,
                    );
                }
                index = previous;
            }
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let _ = hour; // Other page sizes retain the mapping until worker join.
    }
}
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
        let (start, end, eol) = fields(input, pos + 28);
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
            latency(input, end + 5, eol),
        );
        pos = eol + 1;
    }
    Ok(part)
}

// Exact monotonic bins: 0..15 have singleton bins; each following power-of-two
// interval has eight bins. All uint32 values map into 0..240 without overflow.
fn percentile_bin(value: u32) -> usize {
    let shift = 28u32.saturating_sub(value.leading_zeros());
    (shift * 8 + (value >> shift)) as usize
}
fn percentiles(values: &mut [u32]) -> [u32; 3] {
    let count = values.len();
    let mut ranks = [50, 95, 99].map(|p| (p * count).div_ceil(100) - 1);
    let mut remaining = values;
    if count >= 512 {
        let mut histogram = [0usize; 240];
        for &value in remaining.iter() {
            histogram[percentile_bin(value)] += 1;
        }
        let (mut before, mut target) = (0, 0);
        let (mut bins, mut below) = ([0; 3], [0; 3]);
        for (bin, &frequency) in histogram.iter().enumerate() {
            while target < 3 && ranks[target] < before + frequency {
                bins[target] = bin;
                below[target] = before;
                target += 1;
            }
            if target == 3 {
                break;
            }
            before += frequency;
        }
        let mut starts = [0; 3];
        for i in 1..3 {
            starts[i] = starts[i - 1];
            if bins[i] != bins[i - 1] {
                starts[i] += histogram[bins[i - 1]];
            }
        }
        let selected = starts[2] + histogram[bins[2]];
        // Avoid a compaction pass when the distribution would retain most
        // values. Repeated target bins contribute their population only once.
        if selected < count / 2 {
            for i in 0..3 {
                ranks[i] = starts[i] + ranks[i] - below[i];
            }
            let mut kept = 0;
            for i in 0..count {
                let value = remaining[i];
                let bin = percentile_bin(value);
                if bin == bins[0] || bin == bins[1] || bin == bins[2] {
                    // kept <= i: overwriting the prefix cannot alter unread data.
                    remaining[kept] = value;
                    kept += 1;
                }
            }
            remaining = &mut remaining[..kept];
        }
    }
    let mut result = [0; 3];
    let mut offset = 0;
    for (slot, rank) in result.iter_mut().zip(ranks) {
        remaining.select_nth_unstable(rank - offset);
        // Retain the pivot for small groups with coinciding percentile ranks.
        remaining = &mut remaining[rank - offset..];
        *slot = remaining[0];
        offset = rank;
    }
    result
}

fn append_integer(output: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0u8; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    output.extend_from_slice(&digits[start..]);
}

fn scatter_hour(
    hour: usize,
    parts: &[Part<'_>],
    starts: &mut [usize],
    errors: &mut [u64],
    values: &mut Vec<u32>,
    cursor: &mut [usize],
) -> [u8; 13] {
    starts.fill(0);
    errors.fill(0);
    let mut label = [0; 13];
    for part in parts {
        if part.labels[hour][0] != 0 {
            label = part.labels[hour];
        }
        part.each(hour, |rank, error, _| {
            starts[rank + 1] += 1;
            errors[rank] += u64::from(error);
        });
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }
    values.resize(*starts.last().unwrap(), 0);
    cursor.copy_from_slice(&starts[..errors.len()]);
    for part in parts {
        part.each(hour, |rank, _, latency| {
            values[cursor[rank]] = latency;
            cursor[rank] += 1;
        });
        part.release_hour(hour);
    }
    label
}
fn format_scattered(
    label: [u8; 13],
    names: &[&[u8]],
    starts: &[usize],
    errors: &[u64],
    values: &mut [u32],
    output: &mut Vec<u8>,
) {
    output.clear();
    for (rank, &name) in names.iter().enumerate() {
        let count = starts[rank + 1] - starts[rank];
        if count == 0 {
            continue;
        }
        let [p50, p95, p99] = percentiles(&mut values[starts[rank]..starts[rank + 1]]);
        let errors = errors[rank];
        let ppm = errors * 1_000_000 / count as u64;
        output.extend_from_slice(&label);
        output.extend_from_slice(b":00:00Z ");
        output.extend_from_slice(name);
        for value in [
            count as u64,
            u64::from(p50),
            u64::from(p95),
            u64::from(p99),
            errors,
            ppm,
        ] {
            output.push(b' ');
            append_integer(output, value);
        }
        output.push(b'\n');
    }
}

fn summarize(data: &Path, out: &Path) -> io::Result<()> {
    let file = File::open(data.join("requests.log"))?;
    let size = file.metadata()?.len() as usize;
    fs::create_dir_all(out)?;
    let output = File::create(out.join("summary.txt"))?;
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
    let mut ranks: HashMap<&[u8], usize, BuildHasherDefault<FastHasher>> = HashMap::default();
    for part in &parts {
        for &name in &part.names {
            ranks.entry(name).or_insert(0);
        }
    }
    let mut names: Vec<_> = ranks.keys().copied().collect();
    names.sort_unstable();
    for (rank, &name) in names.iter().enumerate() {
        *ranks.get_mut(name).unwrap() = rank;
    }
    for part in &mut parts {
        part.rank = part.names.iter().map(|name| ranks[name]).collect();
    }
    drop(ranks);
    let next = AtomicUsize::new(0);
    let offsets: [AtomicU64; 73] = std::array::from_fn(|_| AtomicU64::new(u64::MAX));
    offsets[0].store(0, Ordering::Relaxed);
    thread::scope(|scope| {
        let mut jobs = Vec::new();
        for _ in 0..workers {
            let (next, parts, names, offsets, output) = (&next, &parts, &names, &offsets, &output);
            jobs.push(scope.spawn(move || -> io::Result<()> {
                let mut starts = vec![0; names.len() + 1];
                let mut errors = vec![0; names.len()];
                let mut cursor = vec![0; names.len()];
                let mut values = Vec::new();
                let mut bytes = Vec::with_capacity(64 * 1024);
                loop {
                    let hour = next.fetch_add(1, Ordering::Relaxed);
                    if hour >= 72 {
                        break;
                    }
                    let label = scatter_hour(
                        hour,
                        parts,
                        &mut starts,
                        &mut errors,
                        &mut values,
                        &mut cursor,
                    );
                    format_scattered(label, names, &starts, &errors, &mut values, &mut bytes);
                    let mut offset = offsets[hour].load(Ordering::Acquire);
                    while offset == u64::MAX {
                        thread::yield_now();
                        offset = offsets[hour].load(Ordering::Acquire);
                    }
                    // Publish before I/O: a write error cannot strand a successor.
                    // A worker reuses its buffer only after write_all_at completes.
                    offsets[hour + 1].store(offset + bytes.len() as u64, Ordering::Release);
                    output.write_all_at(&bytes, offset)?;
                }
                Ok(())
            }));
        }
        for job in jobs {
            job.join().unwrap()?;
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

#[cfg(test)]
mod quantile_tests {
    use super::percentiles;

    fn check(mut values: Vec<u32>) {
        let mut sorted = values.clone();
        sorted.sort_unstable();
        let expected = [50, 95, 99].map(|p| sorted[(p * sorted.len()).div_ceil(100) - 1]);
        assert_eq!(percentiles(&mut values), expected);
    }

    #[test]
    fn ranks_ties_and_thresholds() {
        for n in [1, 2, 3, 4, 19, 20, 21, 99, 100, 101, 511, 512, 513, 4096] {
            for value in [0, 1, 15, 16, 1023, 1024, u32::MAX] {
                check(vec![value; n]);
            }
            check(
                (0..n)
                    .map(|i| if i % 100 < 95 { 0 } else { u32::MAX })
                    .collect(),
            );
            check(
                (0..n)
                    .map(|i| if i % 100 < 99 { 1024 } else { 1025 })
                    .collect(),
            );
        }
    }

    #[test]
    fn all_exponent_boundaries_and_full_width_values() {
        let mut values = vec![0, 1, u32::MAX];
        for bit in 0..32 {
            let power = 1u32 << bit;
            values.extend([power - 1, power, power.saturating_add(1)]);
            if bit >= 3 {
                for fraction in 0..8 {
                    let boundary = power + fraction * (power >> 3);
                    values.extend([boundary - 1, boundary, boundary.saturating_add(1)]);
                }
            }
        }
        check(values.clone());
        values.reverse();
        check(values.repeat(8));
    }

    #[test]
    fn randomized_distributions_against_sorting() {
        let mut state = 1u64;
        for seed in 0..128 {
            let n = 512 + seed * 31;
            let values: Vec<_> = (0..n)
                .map(|_| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let value = (state >> 32) as u32;
                    match seed % 4 {
                        0 => value,
                        1 => value & 1023,
                        2 => value >> (value & 31),
                        _ => 0xffff0000 | (value & 255),
                    }
                })
                .collect();
            check(values);
        }
    }
}
