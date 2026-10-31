use std::collections::HashMap;
use std::env;
use std::ffi::c_void;
use std::fs::{self, File};
use std::hash::{BuildHasherDefault, Hasher};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
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
    fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len() as usize;
        if size == 0 {
            return Ok(Self(std::ptr::null_mut(), 0));
        }
        let ptr = unsafe { mmap(std::ptr::null_mut(), size, 1, 2, file.as_raw_fd(), 0) };
        if ptr as isize == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(ptr, size))
    }
    fn bytes(&self) -> &[u8] {
        if self.1 == 0 {
            return &[];
        }
        // Read-only mapping; this borrow and all scoped workers end before unmapping.
        unsafe { std::slice::from_raw_parts(self.0.cast(), self.1) }
    }
}
impl Drop for Mapping {
    fn drop(&mut self) {
        if self.1 != 0 {
            unsafe {
                munmap(self.0, self.1);
            }
        }
    }
}

#[derive(Default)]
struct Fast(u64);
impl Hasher for Fast {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() == 16 || bytes.len() == 32 {
            let word = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            let mut value = word(0) ^ word(8).rotate_left(13);
            if bytes.len() == 32 {
                value ^= word(16).rotate_left(27) ^ word(24).rotate_left(41);
            }
            self.0 = value.wrapping_mul(0x517cc1b727220a95);
            self.0 ^= self.0 >> 32;
        } else {
            for &byte in bytes {
                self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x517cc1b727220a95);
            }
        }
    }
    fn write_u64(&mut self, value: u64) {
        let value = (value ^ (value >> 32)).wrapping_mul(0x517cc1b727220a95);
        self.0 = value ^ (value >> 32);
    }
}
type Table<K, V> = HashMap<K, V, BuildHasherDefault<Fast>>;

#[derive(Default)]
struct Dictionary {
    short: Table<[u8; 16], usize>,
    long: Table<[u8; 32], usize>,
}
impl Dictionary {
    fn len(&self) -> usize {
        self.short.len() + self.long.len()
    }
    fn get(&self, key: &[u8; 32], len: usize) -> Option<&usize> {
        if len <= 16 {
            self.short.get(<&[u8; 16]>::try_from(&key[..16]).unwrap())
        } else {
            self.long.get(key)
        }
    }
    fn insert(&mut self, key: [u8; 32], len: usize, id: usize) {
        if len <= 16 {
            self.short
                .insert(<[u8; 16]>::try_from(&key[..16]).unwrap(), id);
        } else {
            self.long.insert(key, id);
        }
    }
}

struct Query {
    terms: Vec<usize>,
    ops: Vec<u8>,
    phrase: Option<usize>,
}
#[derive(Clone, Default)]
struct Summary {
    sum: u64,
    count: u32,
    first: [u32; 10],
}
impl Summary {
    fn write(&self, output: &mut Vec<u8>) {
        write!(output, "{} {}", self.count, self.sum).unwrap();
        for id in &self.first[..self.count.min(10) as usize] {
            write!(output, " {id}").unwrap();
        }
        output.push(b'\n');
    }
}
struct Clause {
    query: usize,
    anchor: usize,
    terms: [u32; 5],
    len: usize,
}
fn anchors(
    input: &[u8],
    dictionary: &Dictionary,
    queries: &[Query],
) -> (Vec<Vec<usize>>, Vec<Clause>, Vec<u32>) {
    let mut frequencies = vec![0u32; dictionary.len()];
    let mut seen = vec![u32::MAX; dictionary.len()];
    for (doc, line) in input.split(|&b| b == b'\n').take(1000).enumerate() {
        let Some(tab) = line.iter().position(|&b| b == b'\t') else {
            continue;
        };
        for word in line[tab + 1..]
            .split(|b| !b.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
        {
            let mut key = [0; 32];
            for (dst, &byte) in key.iter_mut().zip(word) {
                *dst = byte.to_ascii_lowercase();
            }
            if let Some(&term) = dictionary.get(&key, word.len())
                && seen[term] != doc as u32
            {
                seen[term] = doc as u32;
                frequencies[term] += 1;
            }
        }
    }
    let mut anchors = vec![Vec::new(); dictionary.len()];
    let mut all = Vec::new();
    for (id, query) in queries
        .iter()
        .enumerate()
        .filter(|(_, q)| q.phrase.is_none())
    {
        let mut clauses = vec![vec![(query.terms[0], true)]];
        for (&op, &term) in query.ops.iter().zip(&query.terms[1..]) {
            if op == b'O' {
                clauses.push(vec![(term, true)]);
            } else {
                for clause in &mut clauses {
                    clause.push((term, op == b'A'));
                }
            }
        }
        for mut terms in clauses {
            terms.sort_unstable();
            terms.dedup();
            if terms.windows(2).any(|pair| pair[0].0 == pair[1].0) {
                continue;
            }
            let anchor = terms
                .iter()
                .filter(|(_, positive)| *positive)
                .min_by_key(|(term, _)| frequencies[*term])
                .unwrap()
                .0;
            terms.retain(|&(term, positive)| term != anchor || !positive);
            terms.sort_unstable_by_key(|&(term, positive)| {
                if positive {
                    frequencies[term]
                } else {
                    1000 - frequencies[term]
                }
            });
            anchors[anchor].push(all.len());
            // A Boolean query has at most six terms; the positive anchor is implicit.
            let mut packed = [0; 5];
            for (slot, &(term, positive)) in packed.iter_mut().zip(&terms) {
                *slot = u32::try_from(term).unwrap() * 2 + u32::from(positive);
            }
            all.push(Clause {
                query: id,
                anchor,
                terms: packed,
                len: terms.len(),
            });
        }
    }
    (anchors, all, frequencies)
}

#[derive(Clone, Default)]
struct Compact {
    sum: u64,
    count: u32,
    first: u32,
}
struct Partial {
    stats: Vec<Compact>,
    first: Vec<[u32; 10]>,
}
impl Partial {
    fn new(len: usize) -> Self {
        Self {
            stats: vec![Compact::default(); len],
            first: Vec::new(),
        }
    }
    fn add(&mut self, query: usize, doc: u32) {
        let stat = &mut self.stats[query];
        if stat.count == 0 {
            stat.first = self.first.len() as u32;
            self.first.push([0; 10]);
        }
        if stat.count < 10 {
            self.first[stat.first as usize][stat.count as usize] = doc;
        }
        stat.count += 1;
        stat.sum += u64::from(doc);
    }
}
#[allow(clippy::too_many_arguments)]
fn flush(
    base: u32,
    bits: &mut [u64],
    touched_terms: &mut Vec<usize>,
    candidates: &[usize],
    clauses: &[Clause],
    query_bits: &mut [u64],
    touched: &mut Vec<usize>,
    results: &mut Partial,
) {
    touched.clear();
    for &id in candidates {
        let clause = &clauses[id];
        let mut value = bits[clause.anchor];
        for &term in &clause.terms[..clause.len] {
            let mask = bits[(term >> 1) as usize];
            value &= if term & 1 != 0 { mask } else { !mask };
            if value == 0 {
                break;
            }
        }
        if value != 0 {
            if query_bits[clause.query] == 0 {
                touched.push(clause.query);
            }
            query_bits[clause.query] |= value;
        }
    }
    for term in touched_terms.drain(..) {
        bits[term] = 0;
    }
    for &query in touched.iter() {
        let word = query_bits[query];
        query_bits[query] = 0;
        let count = word.count_ones();
        if count <= 4 {
            let mut rest = word;
            while rest != 0 {
                results.add(query, base + rest.trailing_zeros());
                rest &= rest - 1;
            }
        } else {
            let stat = &mut results.stats[query];
            if stat.count == 0 {
                stat.first = results.first.len() as u32;
                results.first.push([0; 10]);
            }
            let mut rest = word;
            let mut i = stat.count as usize;
            while i < 10 && rest != 0 {
                results.first[stat.first as usize][i] = base + rest.trailing_zeros();
                rest &= rest - 1;
                i += 1;
            }
            stat.count += count;
            stat.sum += u64::from(base) * u64::from(count)
                + u64::from((word & 0xaaaaaaaaaaaaaaaa).count_ones())
                + 2 * u64::from((word & 0xcccccccccccccccc).count_ones())
                + 4 * u64::from((word & 0xf0f0f0f0f0f0f0f0).count_ones())
                + 8 * u64::from((word & 0xff00ff00ff00ff00).count_ones())
                + 16 * u64::from((word & 0xffff0000ffff0000).count_ones())
                + 32 * u64::from((word & 0xffffffff00000000).count_ones());
        }
    }
}

fn read_token(input: &[u8], pos: &mut usize, token: &mut [u8; 32]) -> usize {
    let mut length = 0;
    #[cfg(target_arch = "x86_64")]
    while *pos + 16 <= input.len() && length < 32 {
        use std::arch::x86_64::*;
        // x86-64 guarantees SSE2. Bounds above cover the entire unaligned load.
        let count = unsafe {
            let raw = _mm_loadu_si128(input.as_ptr().add(*pos).cast());
            let lower = _mm_or_si128(raw, _mm_set1_epi8(0x20));
            let digits = _mm_and_si128(
                _mm_cmpgt_epi8(raw, _mm_set1_epi8(b'/' as i8)),
                _mm_cmpgt_epi8(_mm_set1_epi8(b':' as i8), raw),
            );
            let letters = _mm_and_si128(
                _mm_cmpgt_epi8(lower, _mm_set1_epi8(b'`' as i8)),
                _mm_cmpgt_epi8(_mm_set1_epi8(b'{' as i8), lower),
            );
            let count =
                (_mm_movemask_epi8(_mm_or_si128(digits, letters)) as u32).trailing_ones() as usize;
            let mask = _mm_cmpgt_epi8(
                _mm_set1_epi8(count as i8),
                _mm_set_epi8(15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0),
            );
            _mm_storeu_si128(
                token.as_mut_ptr().add(length).cast(),
                _mm_and_si128(lower, mask),
            );
            count
        };
        length += count;
        *pos += count;
        if count != 16 {
            return length;
        }
    }
    while *pos < input.len() && input[*pos].is_ascii_alphanumeric() {
        token[length] = input[*pos].to_ascii_lowercase();
        length += 1;
        *pos += 1;
    }
    length
}
fn parse(
    input: &[u8],
    dictionary: &Dictionary,
    anchors: &[Vec<usize>],
    queries: &[Query],
    clauses: &[Clause],
    checks: &[Vec<(usize, usize)>],
) -> Partial {
    let worker_started = std::time::Instant::now();
    let mut flush_elapsed = std::time::Duration::ZERO;
    let mut results = Partial::new(queries.len());
    let mut touched_terms = Vec::new();
    let mut matched = vec![u32::MAX; queries.len()];
    let mut bits = vec![0u64; dictionary.len()];
    let mut query_bits = vec![0u64; queries.len()];
    let mut touched = Vec::new();
    let mut base = 0;
    let mut candidates = Vec::new();

    let mut tokens = Vec::new();
    let mut phrase_candidates = Vec::new();
    let mut pos = 0;
    let mut token = [0u8; 32];
    while pos < input.len() {
        let mut doc = 0u32;
        while input[pos] != b'\t' {
            doc = doc * 10 + u32::from(input[pos] - b'0');
            pos += 1;
        }
        pos += 1;
        let block = doc & !63;
        if block != base {
            let flush_started = std::time::Instant::now();
            flush(
                base,
                &mut bits,
                &mut touched_terms,
                &candidates,
                clauses,
                &mut query_bits,
                &mut touched,
                &mut results,
            );
            flush_elapsed += flush_started.elapsed();
            candidates.clear();
            base = block;
        }
        tokens.clear();
        phrase_candidates.clear();
        while input[pos] != b'\n' {
            token.fill(0);
            let len = read_token(input, &mut pos, &mut token);
            if len == 0 {
                pos += 1;
                continue;
            }
            let term = dictionary.get(&token, len).copied().unwrap_or(usize::MAX);
            tokens.push(term);
            if term != usize::MAX {
                if bits[term] == 0 {
                    touched_terms.push(term);
                    candidates.extend_from_slice(&anchors[term]);
                }
                bits[term] |= 1u64 << (doc - base);
                for &(id, anchor) in &checks[term] {
                    if tokens.len() > anchor {
                        phrase_candidates.push((id, tokens.len() - 1 - anchor));
                    }
                }
            }
            if input[pos] != b'\n' {
                pos += 1;
            }
        }
        for &(id, start) in &phrase_candidates {
            let terms = &queries[id].terms;
            if matched[id] != doc
                && tokens.get(start..start + terms.len()) == Some(terms.as_slice())
            {
                matched[id] = doc;
                results.add(id, doc);
            }
        }
        pos += 1;
    }
    let flush_started = std::time::Instant::now();
    flush(
        base,
        &mut bits,
        &mut touched_terms,
        &candidates,
        clauses,
        &mut query_bits,
        &mut touched,
        &mut results,
    );
    flush_elapsed += flush_started.elapsed();
    eprintln!(
        "worker_ms={:.3} flush_ms={:.3}",
        worker_started.elapsed().as_secs_f64() * 1000.0,
        flush_elapsed.as_secs_f64() * 1000.0
    );
    results
}

fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let query_text = fs::read(data.join("queries.txt"))?;
    let mut dictionary = Dictionary::default();
    let mut queries = Vec::new();
    let mut phrases = 0;
    for line in query_text
        .split(|&b| b == b'\n')
        .filter(|line| !line.is_empty())
    {
        let phrase = line.starts_with(b"PHRASE");
        let rest = if phrase {
            &line[8..line.len() - 1]
        } else {
            &line[5..]
        };
        let mut query = Query {
            terms: Vec::new(),
            ops: Vec::new(),
            phrase: None,
        };
        for (i, word) in rest.split(|&b| b == b' ').enumerate() {
            if !phrase && i % 2 == 1 {
                query.ops.push(word[0]);
                continue;
            }
            let mut key = [0; 32];
            key[..word.len()].copy_from_slice(word);
            let id = match dictionary.get(&key, word.len()) {
                Some(&id) => id,
                None => {
                    let id = dictionary.len();
                    dictionary.insert(key, word.len(), id);
                    id
                }
            };
            query.terms.push(id);
        }
        if phrase {
            query.phrase = Some(phrases);
            phrases += 1;
        }
        queries.push(query);
    }
    let mapping = Mapping::open(&data.join("docs.txt"))?;
    let input = mapping.bytes();
    let (anchors, clauses, frequencies) = anchors(input, &dictionary, &queries);
    let mut checks = vec![Vec::new(); dictionary.len()];
    for (id, query) in queries
        .iter()
        .enumerate()
        .filter(|(_, q)| q.phrase.is_some())
    {
        let anchor = (0..query.terms.len())
            .min_by_key(|&i| frequencies[query.terms[i]])
            .unwrap();
        checks[query.terms[anchor]].push((id, anchor));
    }
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(input.len().div_ceil(64 * 1024).max(1));
    let parsed = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let mut start = 0;
        for worker in 0..workers {
            let mut end = input.len() * (worker + 1) / workers;
            while end < input.len() && end > 0 && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            let dictionary = &dictionary;
            let queries = &queries;
            let anchors = &anchors;
            let clauses = &clauses;
            let checks = &checks;
            jobs.push(
                scope.spawn(move || parse(chunk, dictionary, anchors, queries, clauses, checks)),
            );
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut results = vec![Summary::default(); queries.len()];
    for part in parsed {
        for (result, stat) in results.iter_mut().zip(part.stats) {
            if stat.count == 0 {
                continue;
            }
            let have = result.count.min(10) as usize;
            let take = (10 - have).min(stat.count as usize);
            result.first[have..have + take]
                .copy_from_slice(&part.first[stat.first as usize][..take]);
            result.count += stat.count;
            result.sum += stat.sum;
        }
    }
    let mut bytes = Vec::new();
    for result in results {
        result.write(&mut bytes);
    }
    fs::create_dir_all(out)?;
    let mut output = File::create(out.join("results.txt"))?;
    output.write_all(&bytes)?;
    Ok(())
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
