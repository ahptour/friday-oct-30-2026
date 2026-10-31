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
        if bytes.len() == 32 {
            let word = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
            self.0 = (word(0)
                ^ word(8).rotate_left(13)
                ^ word(16).rotate_left(27)
                ^ word(24).rotate_left(41))
            .wrapping_mul(0x517cc1b727220a95);
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

struct Query {
    terms: Vec<usize>,
    ops: Vec<u8>,
    // Dense phrase ID; usize::MAX marks a Boolean query.
    phrase: usize,
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
    // Dense Boolean ID, independent of original query/output order.
    mask: usize,
    anchor: usize,
    terms: Vec<(usize, bool)>,
}
fn anchors(
    input: &[u8],
    dictionary: &Lexicon,
    queries: &[Query],
) -> (Vec<Vec<usize>>, Vec<Clause>, Vec<u32>) {
    let mut frequencies = vec![0u32; dictionary.keys.len()];
    let mut seen = vec![u32::MAX; dictionary.keys.len()];
    for (doc, line) in input.split(|&b| b == b'\n').take(1000).enumerate() {
        let Some(tab) = line.iter().position(|&b| b == b'\t') else {
            continue;
        };
        let mut pos = tab + 1;
        let mut tokens = Tokens::new(line);
        while let Some((start, end)) = tokens.next(&mut pos) {
            if let Some(term) = dictionary.get(line, start, end)
                && seen[term] != doc as u32
            {
                seen[term] = doc as u32;
                frequencies[term] += 1;
            }
        }
    }
    let mut anchors = vec![Vec::new(); dictionary.keys.len()];
    let mut all = Vec::new();
    for (mask, (id, query)) in queries
        .iter()
        .enumerate()
        .filter(|(_, q)| q.phrase == usize::MAX)
        .enumerate()
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
            all.push(Clause {
                query: id,
                mask,
                anchor,
                terms,
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
        for &(term, positive) in &clause.terms {
            let mask = bits[term];
            value &= if positive { mask } else { !mask };
            if value == 0 {
                break;
            }
        }
        if value != 0 {
            if query_bits[clause.mask] == 0 {
                touched.push(id);
            }
            query_bits[clause.mask] |= value;
        }
    }
    for term in touched_terms.drain(..) {
        bits[term] = 0;
    }
    for &id in touched.iter() {
        let clause = &clauses[id];
        let query = clause.query;
        let word = query_bits[clause.mask];
        query_bits[clause.mask] = 0;
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

struct Tokens<'a> {
    input: &'a [u8],
    base: usize,
    alnum: u64,
    newlines: u64,
}
impl<'a> Tokens<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            base: usize::MAX,
            alnum: 0,
            newlines: 0,
        }
    }
    fn at(&mut self, pos: usize) -> usize {
        let base = pos & !63;
        if self.base != base {
            self.base = base;
            self.alnum = 0;
            self.newlines = 0;
            self.load();
        }
        pos - base
    }
    fn load(&mut self) {
        #[cfg(target_arch = "x86_64")]
        if self.base + 64 <= self.input.len() {
            use std::arch::x86_64::*;
            // Four bounded SSE2 loads classify a block once, regardless of
            // how many tokens it contains. High UTF-8 bytes are separators.
            for lane in 0..4 {
                unsafe {
                    let raw =
                        _mm_loadu_si128(self.input.as_ptr().add(self.base + lane * 16).cast());
                    let lower = _mm_or_si128(raw, _mm_set1_epi8(0x20));
                    let digits = _mm_and_si128(
                        _mm_cmpgt_epi8(raw, _mm_set1_epi8(b'/' as i8)),
                        _mm_cmpgt_epi8(_mm_set1_epi8(b':' as i8), raw),
                    );
                    let letters = _mm_and_si128(
                        _mm_cmpgt_epi8(lower, _mm_set1_epi8(b'`' as i8)),
                        _mm_cmpgt_epi8(_mm_set1_epi8(b'{' as i8), lower),
                    );
                    self.alnum |=
                        (_mm_movemask_epi8(_mm_or_si128(digits, letters)) as u64) << (lane * 16);
                    self.newlines |=
                        (_mm_movemask_epi8(_mm_cmpeq_epi8(raw, _mm_set1_epi8(b'\n' as i8))) as u64)
                            << (lane * 16);
                }
            }
            return;
        }
        for (i, &byte) in self.input[self.base..].iter().take(64).enumerate() {
            self.alnum |= u64::from(byte.is_ascii_alphanumeric()) << i;
            self.newlines |= u64::from(byte == b'\n') << i;
        }
    }
    fn next(&mut self, pos: &mut usize) -> Option<(usize, usize)> {
        loop {
            if *pos == self.input.len() {
                return None;
            }
            let offset = self.at(*pos);
            let available = (self.input.len() - *pos).min(64 - offset);
            let start = (self.alnum >> offset).trailing_zeros() as usize;
            let newline = (self.newlines >> offset).trailing_zeros() as usize;
            if newline < available && newline < start {
                *pos += newline;
                return None;
            }
            if start < available {
                *pos += start;
                break;
            }
            *pos += available;
        }
        let start = *pos;
        loop {
            let offset = self.at(*pos);
            let available = (self.input.len() - *pos).min(64 - offset);
            let count = ((self.alnum >> offset).trailing_ones() as usize).min(available);
            *pos += count;
            if count < available || *pos == self.input.len() {
                return Some((start, *pos));
            }
        }
    }
}
fn lower_word(input: &[u8], start: usize, count: usize) -> u64 {
    let word = if start + 8 <= input.len() {
        u64::from_le_bytes(input[start..start + 8].try_into().unwrap())
    } else {
        let mut word = 0;
        for (i, &byte) in input[start..start + count].iter().enumerate() {
            word |= u64::from(byte) << (i * 8);
        }
        word
    };
    // Called only on alphanumeric token bytes: OR 0x20 lowercases letters and
    // leaves digits alone. Mask any separators read beyond a short token.
    (word | 0x2020202020202020) & (u64::MAX >> ((8 - count) * 8))
}
struct Lexicon {
    slots: Vec<u64>,
    keys: Vec<[u8; 32]>,
    lengths: Vec<u8>,
}
impl Lexicon {
    fn hash(head: u64, tail: u64, len: usize) -> u64 {
        let h =
            (head ^ tail.rotate_left(27) ^ ((len as u64) << 56)).wrapping_mul(0x517cc1b727220a95);
        h ^ (h >> 32)
    }
    fn new(dictionary: &Table<[u8; 32], usize>) -> Self {
        let mut result = Self {
            slots: vec![0; (dictionary.len() * 2).next_power_of_two().max(1)],
            keys: vec![[0; 32]; dictionary.len()],
            lengths: vec![0; dictionary.len()],
        };
        for (&key, &id) in dictionary {
            let len = key.iter().position(|&b| b == 0).unwrap_or(32);
            let head = lower_word(&key, 0, len.min(8));
            let tail = if len > 8 {
                lower_word(&key, len - 8, 8)
            } else {
                0
            };
            let hash = Self::hash(head, tail, len);
            let mut slot = hash as usize & (result.slots.len() - 1);
            while result.slots[slot] != 0 {
                slot = (slot + 1) & (result.slots.len() - 1);
            }
            result.slots[slot] = (hash & 0xffffffff00000000)
                | u64::from(u32::try_from(id + 1).expect("query vocabulary"));
            result.keys[id] = key;
            result.lengths[id] = len as u8;
        }
        result
    }
    fn get(&self, input: &[u8], start: usize, end: usize) -> Option<usize> {
        let len = end - start;
        if len == 0 || len > 32 {
            return None;
        }
        let head = lower_word(input, start, len.min(8));
        let tail = if len > 8 {
            lower_word(input, end - 8, 8)
        } else {
            0
        };
        let hash = Self::hash(head, tail, len);
        let mut slot = hash as usize & (self.slots.len() - 1);
        loop {
            let entry = self.slots[slot];
            if entry == 0 {
                return None;
            }
            if entry >> 32 == hash >> 32 {
                let id = (entry as u32 - 1) as usize;
                let key = &self.keys[id];
                if self.lengths[id] as usize == len
                    && lower_word(key, 0, len.min(8)) == head
                    && (len <= 8 || lower_word(key, len - 8, 8) == tail)
                    && (8..len.saturating_sub(8)).step_by(8).all(|offset| {
                        lower_word(input, start + offset, 8) == lower_word(key, offset, 8)
                    })
                {
                    return Some(id);
                }
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }
}

fn parse(
    input: &[u8],
    dictionary: &Lexicon,
    anchors: &[Vec<usize>],
    queries: &[Query],
    clauses: &[Clause],
    checks: &[Vec<(usize, usize)>],
) -> Partial {
    let mut results = Partial::new(queries.len());
    let mut touched_terms = Vec::new();
    let phrases = queries.iter().filter(|q| q.phrase != usize::MAX).count();
    let mut matched = vec![u32::MAX; phrases];
    let mut bits = vec![0u64; dictionary.keys.len()];
    let mut query_bits = vec![0u64; queries.len() - phrases];
    let mut touched = Vec::new();
    let mut base = 0;
    let mut candidates = Vec::new();

    let mut tokens = Vec::new();
    let mut phrase_candidates = Vec::new();
    let mut pos = 0;
    let mut scanner = Tokens::new(input);
    while pos < input.len() {
        let mut doc = 0u32;
        while input[pos] != b'\t' {
            doc = doc * 10 + u32::from(input[pos] - b'0');
            pos += 1;
        }
        pos += 1;
        let block = doc & !63;
        if block != base {
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
            candidates.clear();
            base = block;
        }
        tokens.clear();
        phrase_candidates.clear();
        while let Some((start, end)) = scanner.next(&mut pos) {
            let term = dictionary.get(input, start, end).unwrap_or(usize::MAX);
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
        }
        for &(id, start) in &phrase_candidates {
            let query = &queries[id];
            let terms = &query.terms;
            if matched[query.phrase] != doc
                && tokens.get(start..start + terms.len()) == Some(terms.as_slice())
            {
                matched[query.phrase] = doc;
                results.add(id, doc);
            }
        }
        pos += 1;
    }
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
    results
}

fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let query_text = fs::read(data.join("queries.txt"))?;
    let mut dictionary: Table<[u8; 32], usize> = Table::default();
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
            phrase: usize::MAX,
        };
        for (i, word) in rest.split(|&b| b == b' ').enumerate() {
            if !phrase && i % 2 == 1 {
                query.ops.push(word[0]);
                continue;
            }
            let mut key = [0; 32];
            key[..word.len()].copy_from_slice(word);
            let id = match dictionary.get(&key) {
                Some(&id) => id,
                None => {
                    let id = dictionary.len();
                    dictionary.insert(key, id);
                    id
                }
            };
            query.terms.push(id);
        }
        if phrase {
            query.phrase = phrases;
            phrases += 1;
        }
        queries.push(query);
    }
    let lexicon = Lexicon::new(&dictionary);
    drop(dictionary);
    let dictionary = lexicon;
    let mapping = Mapping::open(&data.join("docs.txt"))?;
    let input = mapping.bytes();
    let (anchors, clauses, frequencies) = anchors(input, &dictionary, &queries);
    let mut checks = vec![Vec::new(); dictionary.keys.len()];
    for (id, query) in queries
        .iter()
        .enumerate()
        .filter(|(_, q)| q.phrase != usize::MAX)
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
