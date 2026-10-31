use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
        // The read-only mapping remains owned until all scoped workers finish.
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
    while pos < input.len() && input[pos] != byte {
        pos += 1;
    }
    pos
}

fn number(input: &[u8], pos: &mut usize, delimiter: u8) -> u64 {
    let mut n = 0;
    while input[*pos] != delimiter {
        n = n * 10 + u64::from(input[*pos] - b'0');
        *pos += 1;
    }
    *pos += 1;
    n
}

// Each classifier reads exactly 64 valid bytes; the final block is copied
// into a padded stack buffer. Prefix parity carries across blocks, including
// doubled quotes split between blocks and embedded newlines inside notes.
type Classify = unsafe fn(*const u8) -> (u64, u64, u64);
unsafe fn scalar_masks(ptr: *const u8) -> (u64, u64, u64) {
    let (mut quotes, mut separators) = (0, 0);
    for i in 0..64 {
        let byte = unsafe { *ptr.add(i) };
        quotes |= u64::from(byte == b'"') << i;
        separators |= u64::from(byte == b',' || byte == b'\n') << i;
    }
    let mut inside = quotes;
    for shift in [1, 2, 4, 8, 16, 32] {
        inside ^= inside << shift;
    }
    (quotes, separators, inside)
}
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,pclmulqdq")]
unsafe fn vector_masks(ptr: *const u8) -> (u64, u64, u64) {
    use std::arch::x86_64::*;
    let a = unsafe { _mm256_loadu_si256(ptr.cast()) };
    let b = unsafe { _mm256_loadu_si256(ptr.add(32).cast()) };
    let mask = |byte| {
        let value = _mm256_set1_epi8(byte);
        u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(a, value)) as u32)
            | (u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(b, value)) as u32) << 32)
    };
    let quotes = mask(b'"' as i8);
    let inside = _mm_clmulepi64_si128(_mm_cvtsi64_si128(quotes as i64), _mm_set1_epi8(-1), 0);
    (
        quotes,
        mask(b',' as i8) | mask(b'\n' as i8),
        _mm_cvtsi128_si64(inside) as u64,
    )
}
fn classifier() -> Classify {
    #[cfg(target_arch = "x86_64")]
    if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("pclmulqdq") {
        return vector_masks;
    }
    scalar_masks
}
struct Structure<'a> {
    input: &'a [u8],
    classify: Classify,
    next: usize,
    base: usize,
    bits: u64,
    quotes: u64,
    before: u64,
    carry: u64,
}
impl<'a> Structure<'a> {
    fn new(input: &'a [u8], start: usize) -> Self {
        Self {
            input,
            classify: classifier(),
            next: start,
            base: start,
            bits: 0,
            quotes: 0,
            before: 0,
            carry: 0,
        }
    }
    fn next(&mut self) -> (usize, u64) {
        while self.bits == 0 {
            if self.next >= self.input.len() {
                return (
                    self.input.len(),
                    self.before + u64::from(self.quotes.count_ones()),
                );
            }
            self.before += u64::from(self.quotes.count_ones());
            self.base = self.next;
            self.next += 64;
            let mut tail;
            let remaining = &self.input[self.base..];
            let ptr = if remaining.len() >= 64 {
                remaining.as_ptr()
            } else {
                tail = [0u8; 64];
                tail[..remaining.len()].copy_from_slice(remaining);
                tail.as_ptr()
            };
            let (quotes, separators, inside) = unsafe { (self.classify)(ptr) };
            let inside = inside ^ self.carry;
            self.carry = 0u64.wrapping_sub(inside >> 63);
            self.quotes = quotes;
            self.bits = separators & !inside;
        }
        let bit = self.bits.trailing_zeros();
        self.bits &= self.bits - 1;
        (
            self.base + bit as usize,
            self.before + u64::from((self.quotes & ((1u64 << bit) - 1)).count_ones()),
        )
    }
}
fn digits(input: &[u8], start: usize, end: usize) -> u64 {
    let len = end - start;
    if (1..=8).contains(&len) && end >= 8 {
        // Read within the complete mapping, including across worker boundaries.
        // Mask preceding field bytes to zero: the eight lanes are now decimal
        // digits with leading zeroes, even for a one-byte field at EOF.
        let v = u64::from_le_bytes(input[end - 8..end].try_into().unwrap());
        let v = v & (0x0f0f0f0f0f0f0f0f << ((8 - len) * 8));
        let v = (v & 0x00ff00ff00ff00ff) * 10 + ((v >> 8) & 0x00ff00ff00ff00ff);
        let v = (v & 0x0000ffff0000ffff) * 100 + ((v >> 16) & 0x0000ffff0000ffff);
        return (v & 0xffffffff) * 10_000 + (v >> 32);
    }
    let input = &input[start..end];
    let mut value = 0;
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
    value
}
fn prefetch<T>(value: &T) {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_mm_prefetch((value as *const T).cast(), std::arch::x86_64::_MM_HINT_T0);
    }
    #[cfg(not(target_arch = "x86_64"))]
    let _ = value;
}

fn load_regions(path: &Path) -> io::Result<(Vec<u8>, Vec<Vec<u8>>)> {
    let data = fs::read(path)?;
    // The frozen contract has at most 240 countries, plus bucket zero.
    let mut lookup = vec![0u8; 1_000_000];
    let mut country_ids = vec![0u8; 26 * 26 * 26];
    let mut keys = vec![b"_UNMATCHED".to_vec()];
    let mut pos = find(&data, 0, b'\n') + 1;
    while pos < data.len() {
        let id = number(&data, &mut pos, b',') as usize;
        let country = &data[pos..pos + 3];
        let code = country
            .iter()
            .fold(0usize, |value, &byte| value * 26 + usize::from(byte - b'A'));
        if country_ids[code] == 0 {
            country_ids[code] = keys.len() as u8;
            keys.push(country.to_vec());
        }
        lookup[id] = country_ids[code];
        pos += 4;
    }
    Ok((lookup, keys))
}

#[derive(Default, Clone)]
struct Group {
    count: u64,
    id_sum: u64,
    sum: i64,
    min: i64,
    max: i64,
    name_bytes: u64,
    note_bytes: u64,
    names_distinct: u64,
}
const NAME_SHARDS: usize = 32;
const NAME_BATCH: usize = 64;
struct Shard<'a> {
    keys: Vec<u64>,
    heads: Vec<[u8; 24]>,
    countries: Vec<[u64; 4]>,
    overflow: HashMap<&'a [u8], [u64; 4], BuildHasherDefault<FastHasher>>,
}
impl<'a> Shard<'a> {
    fn new(capacity: usize) -> Self {
        Self {
            keys: vec![0; capacity],
            heads: vec![[0; 24]; capacity],
            countries: vec![[0; 4]; capacity],
            overflow: HashMap::default(),
        }
    }
    fn intern(&mut self, input: &[u8], name: &[u8], hash: u64) -> Option<usize> {
        let start = name.as_ptr() as usize - input.as_ptr() as usize;
        // Eligibility is mapping-wide, so an early and a late occurrence can
        // never split the same name between compact storage and overflow.
        if input.len() < u32::MAX as usize && name.len() <= u16::MAX as usize {
            // Shard, fingerprint and initial slot use disjoint hash bits.
            let key =
                ((start as u64 + 1) << 32) | (((hash >> 32) & 0xffff) << 16) | name.len() as u64;
            for probe in 0..32 {
                let slot = (hash as usize).wrapping_add(probe) & (self.keys.len() - 1);
                let old = self.keys[slot];
                if old == 0 {
                    self.keys[slot] = key;
                    let len = name.len().min(24);
                    self.heads[slot][..len].copy_from_slice(&name[..len]);
                    return Some(slot);
                }
                if old as u32 == key as u32 {
                    let offset = (old >> 32) as usize - 1;
                    if self.heads[slot][..name.len().min(24)] == name[..name.len().min(24)]
                        && (name.len() <= 24
                            || input[offset + 24..offset + name.len()] == name[24..])
                    {
                        return Some(slot);
                    }
                }
            }
        }
        // Oversized names/mappings and crowded probe windows stay exact.
        None
    }
    fn mark(&mut self, input: &[u8], row: Pending<'a>) -> bool {
        let words = if let Some(slot) = self.intern(input, row.name, row.hash) {
            &mut self.countries[slot]
        } else {
            self.overflow.entry(row.name).or_insert([0; 4])
        };
        let bit = 1u64 << (row.country % 64);
        let word = &mut words[row.country / 64];
        let fresh = *word & bit == 0;
        *word |= bit;
        fresh
    }
}
struct Names<'a> {
    shards: Vec<Mutex<Shard<'a>>>,
}
impl<'a> Names<'a> {
    fn flush(&self, shard: usize, input: &[u8], rows: &mut Vec<Pending<'a>>, groups: &mut [Group]) {
        if rows.is_empty() {
            return;
        }
        // One lock covers the entire batch. Keys, heads and membership words
        // are accessed only by the owner of this shard's mutex.
        let mut table = self.shards[shard].lock().unwrap();
        for i in 0..rows.len() {
            if let Some(next) = rows.get(i + 8) {
                let slot = next.hash as usize & (table.keys.len() - 1);
                prefetch(&table.keys[slot]);
                prefetch(&table.heads[slot]);
                prefetch(&table.countries[slot]);
            }
            let row = rows[i];
            groups[row.country].names_distinct += u64::from(table.mark(input, row));
        }
        rows.clear();
    }
}
#[derive(Clone, Copy)]
struct Pending<'a> {
    name: &'a [u8],
    country: usize,
    hash: u64,
}
struct Pipeline<'a> {
    pending: [Vec<Pending<'a>>; NAME_SHARDS],
}
impl<'a> Pipeline<'a> {
    fn push(&mut self, row: Pending<'a>, names: &Names<'a>, input: &[u8], groups: &mut [Group]) {
        let shard = (row.hash >> 48) as usize & (NAME_SHARDS - 1);
        let rows = &mut self.pending[shard];
        rows.push(row);
        if rows.len() == NAME_BATCH {
            names.flush(shard, input, rows, groups);
        }
    }
    fn finish(&mut self, names: &Names<'a>, input: &[u8], groups: &mut [Group]) {
        for (shard, rows) in self.pending.iter_mut().enumerate() {
            names.flush(shard, input, rows, groups);
        }
    }
}
fn parse<'a>(
    input: &'a [u8],
    names: &Names<'a>,
    start: usize,
    end: usize,
    lookup: &[u8],
    countries: usize,
) -> Vec<Group> {
    let mut groups = vec![Group::default(); countries];
    let mut pipeline = Pipeline {
        pending: std::array::from_fn(|_| Vec::with_capacity(NAME_BATCH)),
    };
    let mut structure = Structure::new(input, start);
    let mut pos = start;
    while pos < end {
        let (f1, q1) = structure.next();
        let (f2, q2) = structure.next();
        let (f3, q3) = structure.next();
        let (f4, _) = structure.next();
        let (eol, _) = structure.next();
        if input[pos] == 0xef {
            pos += 3;
        }
        let id = digits(input, pos, f1);
        let quoted = usize::from(q2 != q1);
        let name = &input[f1 + 1 + quoted..f2 - quoted];
        let region = digits(input, f3 + 1, f4) as usize;
        let country = usize::from(*lookup.get(region).unwrap_or(&0));
        let amount = f4 + 1;
        let negative = usize::from(input[amount] == b'-');
        let amount_end = eol - usize::from(input[eol - 1] == b'\r');
        let cents = (digits(input, amount + negative, amount_end - 3) * 100
            + u64::from(input[amount_end - 2] - b'0') * 10
            + u64::from(input[amount_end - 1] - b'0')) as i64;
        let cents = if negative != 0 { -cents } else { cents };
        let group = &mut groups[country];
        if group.count == 0 {
            group.min = cents;
            group.max = cents;
        }
        group.count += 1;
        group.id_sum += id;
        group.sum += cents;
        group.min = group.min.min(cents);
        group.max = group.max.max(cents);
        group.name_bytes += (f2 - f1 - 1) as u64 - (q2 - q1) / 2 - quoted as u64;
        group.note_bytes += (f3 - f2 - 1) as u64 - (q3 - q2) / 2 - u64::from(q3 != q2);
        let mut hash = FastHasher::default();
        name.hash(&mut hash);
        pipeline.push(
            Pending {
                name,
                country,
                hash: hash.finish(),
            },
            names,
            input,
            &mut groups,
        );
        pos = (eol + 1).min(input.len());
    }
    // Every partial batch is committed before this worker publishes its groups.
    pipeline.finish(names, input, &mut groups);
    groups
}

fn money(cents: i64) -> String {
    format!(
        "{}{}.{:02}",
        if cents < 0 { "-" } else { "" },
        cents.abs() / 100,
        cents.abs() % 100
    )
}

fn finish(keys: &[Vec<u8>], parts: &[Vec<Group>], names: Names<'_>) -> Vec<(Vec<u8>, String)> {
    drop(names);
    let mut output = Vec::new();
    for country in 0..keys.len() {
        let mut total = Group::default();
        for group in parts
            .iter()
            .map(|part| &part[country])
            .filter(|group| group.count > 0)
        {
            if total.count == 0 {
                total.min = group.min;
                total.max = group.max;
            }
            total.count += group.count;
            total.id_sum += group.id_sum;
            total.sum += group.sum;
            total.min = total.min.min(group.min);
            total.max = total.max.max(group.max);
            total.name_bytes += group.name_bytes;
            total.note_bytes += group.note_bytes;
            total.names_distinct += group.names_distinct;
        }
        if total.count > 0 {
            output.push((
                keys[country].clone(),
                format!(
                    " {} {} {} {} {} {} {} {}\n",
                    total.count,
                    total.id_sum,
                    money(total.sum),
                    money(total.min),
                    money(total.max),
                    total.names_distinct,
                    total.name_bytes,
                    total.note_bytes
                ),
            ));
        }
    }
    output
}

fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let (lookup, keys) = load_regions(&data.join("regions.csv"))?;
    let file = File::open(data.join("records.csv"))?;
    let len = file.metadata()?.len() as usize;
    let ptr = unsafe { mmap(std::ptr::null_mut(), len, 1, 2, file.as_raw_fd(), 0) };
    if ptr as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let map = Mapping(ptr, len);
    let input = map.bytes();
    let header_end = find(input, 0, b'\n') + 1;
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(len.div_ceil(64 * 1024));
    let mut bounds = vec![header_end];
    for worker in 1..workers {
        let mut pos = len * worker / workers;
        loop {
            pos = (find(input, pos, b'\n') + 1).min(len);
            // Generator note continuations begin with lowercase text or a quote,
            // never a numeric id or BOM. Names contain no embedded newline.
            if pos == len || input[pos].is_ascii_digit() || input[pos] == 0xef {
                break;
            }
        }
        bounds.push(pos);
    }
    bounds.push(len);
    // Cap the common-name table; full byte equality and the overflow map keep
    // distinct counting exact even when the table is crowded.
    let capacity = (len / 64).next_power_of_two().clamp(1024, 1 << 17);
    let names = Names {
        shards: (0..NAME_SHARDS)
            .map(|_| Mutex::new(Shard::new(capacity / NAME_SHARDS)))
            .collect(),
    };
    let parts = thread::scope(|scope| {
        let jobs: Vec<_> = bounds
            .windows(2)
            .map(|bound| {
                let lookup = &lookup;
                let countries = keys.len();
                let names = &names;
                scope.spawn(move || parse(input, names, bound[0], bound[1], lookup, countries))
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut rows = finish(&keys, &parts, names);
    rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    fs::create_dir_all(out)?;
    let mut output = File::create(out.join("aggregates.txt"))?;
    for (key, values) in rows {
        output.write_all(&key)?;
        output.write_all(values.as_bytes())?;
    }
    Ok(())
}
fn main() -> io::Result<()> {
    let mut data = PathBuf::from("/data");
    let mut out = PathBuf::from("/out");
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data") => data = PathBuf::from(args.next().expect("--data directory")),
            Some("--out") => out = PathBuf::from(args.next().expect("--out directory")),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "usage: solution [--data DIR] [--out DIR]",
                ));
            }
        }
    }
    solve(&data, &out)
}
