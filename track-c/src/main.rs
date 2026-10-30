use std::collections::HashMap;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hash, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
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

fn field<'a>(input: &'a [u8], pos: &mut usize) -> (&'a [u8], u64) {
    let quoted = input[*pos] == b'"';
    *pos += usize::from(quoted);
    let start = *pos;
    let mut escapes = 0;
    if quoted {
        loop {
            *pos = find(input, *pos, b'"');
            if input[*pos + 1] != b'"' {
                break;
            }
            escapes += 1;
            *pos += 2;
        }
    } else {
        *pos = find(input, *pos, b',');
    }
    let length = *pos - start;
    *pos += 1 + usize::from(quoted);
    (&input[start..start + length], length as u64 - escapes)
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
#[derive(Default)]
struct Entry {
    key: AtomicU64,
    countries: [AtomicU64; 4],
}
struct Names<'a> {
    slots: Vec<Entry>,
    overflow: Mutex<HashMap<&'a [u8], [u64; 4], BuildHasherDefault<FastHasher>>>,
}
impl<'a> Names<'a> {
    fn first(&self, input: &[u8], name: &'a [u8], country: usize) -> bool {
        // CSV escaping is canonical, so encoded payload equality preserves decoded
        // name equality. Both empty spellings have an empty payload.
        let bit = 1 << (country % 64);
        let start = name.as_ptr() as usize - input.as_ptr() as usize;
        if start < u32::MAX as usize && name.len() <= u32::MAX as usize {
            let key = ((start as u64 + 1) << 32) | name.len() as u64;
            let mut hash = FastHasher::default();
            name.hash(&mut hash);
            for probe in 0..32 {
                let slot = &self.slots
                    [(hash.finish() as usize).wrapping_add(probe) & (self.slots.len() - 1)];
                // The key points into immutable input that predates every worker;
                // country words are already initialized atomics, not published data.
                let mut old = slot.key.load(Ordering::Relaxed);
                if old == 0 {
                    old = match slot.key.compare_exchange(
                        0,
                        key,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => key,
                        Err(old) => old,
                    };
                }
                let offset = (old >> 32) as usize - 1;
                let length = (old as u32) as usize;
                if input[offset..offset + length] == *name {
                    return slot.countries[country / 64].fetch_or(bit, Ordering::Relaxed) & bit
                        == 0;
                }
            }
        }
        // A bounded probe count keeps pathological collisions from stalling;
        // the fallback still stores and compares complete name bytes.
        let mut overflow = self.overflow.lock().unwrap();
        let words = overflow.entry(name).or_insert([0; 4]);
        let fresh = words[country / 64] & bit == 0;
        words[country / 64] |= bit;
        fresh
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
    let mut pos = start;
    while pos < end {
        if input[pos] == 0xef {
            pos += 3;
        }
        let id = number(input, &mut pos, b',');
        let (name, name_bytes) = field(input, &mut pos);
        let (_, note_bytes) = field(input, &mut pos);
        let region = number(input, &mut pos, b',') as usize;
        let negative = input[pos] == b'-';
        pos += usize::from(negative);
        let whole = number(input, &mut pos, b'.') as i64;
        let cents =
            whole * 100 + i64::from(input[pos] - b'0') * 10 + i64::from(input[pos + 1] - b'0');
        let cents = if negative { -cents } else { cents };
        pos += 2;
        if pos < input.len() && input[pos] == b'\r' {
            pos += 1;
        }
        if pos < input.len() && input[pos] == b'\n' {
            pos += 1;
        }
        let country = usize::from(*lookup.get(region).unwrap_or(&0));
        let fresh = names.first(input, name, country);
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
        group.names_distinct += u64::from(fresh);
        group.name_bytes += name_bytes;
        group.note_bytes += note_bytes;
    }
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
    let names = Names {
        slots: std::iter::repeat_with(Entry::default)
            .take((len / 64).next_power_of_two().clamp(1024, 1 << 17))
            .collect(),
        overflow: Mutex::new(HashMap::default()),
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
