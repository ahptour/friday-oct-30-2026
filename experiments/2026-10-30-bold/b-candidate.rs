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
    clauses: Vec<Vec<(usize, bool)>>,
    phrase: bool,
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
// Each local posting promotes when its document vector exceeds bitmap payload.
// Dense words use u32 so both representations share one allocation type.
#[derive(Default)]
struct Posting {
    data: Vec<u32>,
    count: u32,
    dense: bool,
}
impl Posting {
    fn insert(&mut self, doc: u32, bitmap_words: usize) {
        self.count += 1;
        if self.dense {
            self.data[doc as usize / 32] |= 1 << (doc % 32);
        } else {
            self.data.push(doc);
            if self.data.len() > bitmap_words {
                let mut bits = vec![0; bitmap_words];
                for &id in &self.data {
                    bits[id as usize / 32] |= 1 << (id % 32);
                }
                self.data = bits;
                self.dense = true;
            }
        }
    }
    fn contains(&self, doc: u32) -> bool {
        if self.dense {
            self.data[doc as usize / 32] & (1 << (doc % 32)) != 0
        } else {
            self.data.binary_search(&doc).is_ok()
        }
    }
    fn word(&self, block: usize, cursor: &mut usize) -> u64 {
        if self.dense {
            return u64::from(self.data[block * 2])
                | (u64::from(self.data.get(block * 2 + 1).copied().unwrap_or(0)) << 32);
        }
        let start = (block * 64) as u32;
        while *cursor < self.data.len() && self.data[*cursor] < start {
            *cursor += 1;
        }
        let mut bits = 0;
        while *cursor < self.data.len() && self.data[*cursor] - start < 64 {
            bits |= 1 << (self.data[*cursor] % 64);
            *cursor += 1;
        }
        bits
    }
    fn visit(&self, mut f: impl FnMut(u32)) {
        if self.dense {
            for (word, &bits) in self.data.iter().enumerate() {
                let mut bits = bits;
                while bits != 0 {
                    f((word * 32) as u32 + bits.trailing_zeros());
                    bits &= bits - 1;
                }
            }
        } else {
            for &doc in &self.data {
                f(doc);
            }
        }
    }
}
fn clauses(terms: &[usize], ops: &[u8]) -> Vec<Vec<(usize, bool)>> {
    let mut result = vec![vec![(terms[0], true)]];
    for (&op, &term) in ops.iter().zip(&terms[1..]) {
        if op == b'O' {
            result.push(vec![(term, true)]);
        } else {
            for clause in &mut result {
                clause.push((term, op == b'A'));
            }
        }
    }
    for clause in &mut result {
        clause.sort_unstable();
        clause.dedup();
    }
    result.retain(|clause| !clause.windows(2).any(|pair| pair[0].0 == pair[1].0));
    result
}
fn newline(input: &[u8], mut pos: usize) -> usize {
    while pos + 8 <= input.len() {
        let word = u64::from_le_bytes(input[pos..pos + 8].try_into().unwrap()) ^ 0x0a0a0a0a0a0a0a0a;
        let mask = word.wrapping_sub(0x0101010101010101) & !word & 0x8080808080808080;
        if mask != 0 {
            return pos + mask.trailing_zeros() as usize / 8;
        }
        pos += 8;
    }
    while pos < input.len() && input[pos] != b'\n' {
        pos += 1;
    }
    pos
}
fn phrase_matches(input: &[u8], terms: &[usize], keys: &[[u8; 32]], failure: &[usize]) -> bool {
    let mut pos = 0;
    let mut matched = 0;
    let mut token = [0; 32];
    while pos < input.len() {
        if !input[pos].is_ascii_alphanumeric() {
            pos += 1;
            continue;
        }
        token.fill(0);
        read_token(input, &mut pos, &mut token);
        while matched > 0 && token != keys[terms[matched]] {
            matched = failure[matched - 1];
        }
        if token == keys[terms[matched]] {
            matched += 1;
            if matched == terms.len() {
                return true;
            }
        }
    }
    false
}
fn union_word(word: usize, value: u64, bits: &mut [u64], touched: &mut Vec<usize>) {
    if value != 0 {
        if bits[word] == 0 {
            touched.push(word);
        }
        bits[word] |= value;
    }
}
fn summarize_word(results: &mut Partial, query: usize, base: u32, word: u64) {
    let count = word.count_ones();
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
fn parse(
    input: &[u8],
    dictionary: &Table<[u8; 32], usize>,
    keys: &[[u8; 32]],
    queries: &[Query],
) -> Partial {
    let mut results = Partial::new(queries.len());
    if input.is_empty() || queries.is_empty() {
        return results;
    }
    // Frozen docs.txt section 2.1 guarantees dense ascending IDs. Each
    // line-aligned chunk therefore maps local document indices to base + index.
    let mut ranges = Vec::new();
    let mut pos = 0;
    let mut base = 0;
    while pos < input.len() {
        let mut doc = 0;
        while input[pos] != b'\t' {
            doc = doc * 10 + u32::from(input[pos] - b'0');
            pos += 1;
        }
        if ranges.is_empty() {
            base = doc;
        }
        let start = pos + 1;
        let end = newline(input, start);
        ranges.push((start, end));
        pos = end + 1;
    }
    let mut postings: Vec<_> = std::iter::repeat_with(Posting::default)
        .take(dictionary.len())
        .collect();
    let mut seen = vec![u32::MAX; dictionary.len()];
    let bitmap_words = ranges.len().div_ceil(32);
    let mut token = [0; 32];
    for (doc, &(start, end)) in ranges.iter().enumerate() {
        let text = &input[start..end];
        let mut pos = 0;
        while pos < text.len() {
            if !text[pos].is_ascii_alphanumeric() {
                pos += 1;
                continue;
            }
            token.fill(0);
            read_token(text, &mut pos, &mut token);
            if let Some(&term) = dictionary.get(&token)
                && seen[term] != doc as u32
            {
                seen[term] = doc as u32;
                postings[term].insert(doc as u32, bitmap_words);
            }
        }
    }
    drop(seen);
    let mut bits = vec![0u64; ranges.len().div_ceil(64)];
    let mut touched = Vec::new();
    for (id, query) in queries.iter().enumerate() {
        if query.phrase {
            let anchor = *query
                .terms
                .iter()
                .min_by_key(|&&term| postings[term].count)
                .unwrap();
            let mut failure = [0; 8];
            for i in 1..query.terms.len() {
                let mut j = failure[i - 1];
                while j > 0 && query.terms[i] != query.terms[j] {
                    j = failure[j - 1];
                }
                if query.terms[i] == query.terms[j] {
                    j += 1;
                }
                failure[i] = j;
            }
            postings[anchor].visit(|doc| {
                if query.terms.iter().all(|&term| postings[term].contains(doc)) {
                    let (start, end) = ranges[doc as usize];
                    if phrase_matches(&input[start..end], &query.terms, keys, &failure) {
                        results.add(id, base + doc);
                    }
                }
            });
            continue;
        }
        for clause in &query.clauses {
            let anchor = clause
                .iter()
                .filter(|(_, positive)| *positive)
                .min_by_key(|(term, _)| postings[*term].count)
                .unwrap()
                .0;
            let posting = &postings[anchor];
            if posting.count == 0 {
                continue;
            }
            if posting.dense {
                let mut cursors = [0; 6];
                let mut anchor_cursor = 0;
                for word in 0..bits.len() {
                    let mut value = posting.word(word, &mut anchor_cursor);
                    for (i, &(term, positive)) in clause.iter().enumerate() {
                        if value == 0 {
                            break;
                        }
                        if term != anchor {
                            let mask = postings[term].word(word, &mut cursors[i]);
                            value &= if positive { mask } else { !mask };
                        }
                    }
                    union_word(word, value, &mut bits, &mut touched);
                }
            } else {
                for &doc in &posting.data {
                    if clause.iter().all(|&(term, positive)| {
                        term == anchor || postings[term].contains(doc) == positive
                    }) {
                        union_word(doc as usize / 64, 1 << (doc % 64), &mut bits, &mut touched);
                    }
                }
            }
        }
        touched.sort_unstable();
        for word in touched.drain(..) {
            summarize_word(&mut results, id, base + (word * 64) as u32, bits[word]);
            bits[word] = 0;
        }
    }
    results
}
fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let query_text = fs::read(data.join("queries.txt"))?;
    let mut dictionary: Table<[u8; 32], usize> = Table::default();
    let mut keys = Vec::new();
    let mut queries = Vec::new();
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
        let mut terms = Vec::new();
        let mut ops = Vec::new();
        for (i, word) in rest.split(|&b| b == b' ').enumerate() {
            if !phrase && i % 2 == 1 {
                ops.push(word[0]);
                continue;
            }
            let mut key = [0; 32];
            key[..word.len()].copy_from_slice(word);
            let id = match dictionary.get(&key) {
                Some(&id) => id,
                None => {
                    let id = keys.len();
                    keys.push(key);
                    dictionary.insert(key, id);
                    id
                }
            };
            terms.push(id);
        }
        let clauses = if phrase {
            Vec::new()
        } else {
            clauses(&terms, &ops)
        };
        queries.push(Query {
            terms,
            clauses,
            phrase,
        });
    }
    let mapping = Mapping::open(&data.join("docs.txt"))?;
    let input = mapping.bytes();
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
            let keys = &keys;
            let queries = &queries;
            jobs.push(scope.spawn(move || parse(chunk, dictionary, keys, queries)));
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
    fs::write(out.join("results.txt"), bytes)
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
