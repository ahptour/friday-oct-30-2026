use std::collections::{HashMap, VecDeque};
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
    phrase: Option<usize>,
}
#[derive(Default)]
struct Node {
    terminal: u32,
    fail: u32,
    output: u32,
}
struct Automaton {
    nodes: Vec<Node>,
    edges: Table<u64, usize>,
    root: Vec<usize>,
    next_query: Vec<u32>,
}
impl Automaton {
    fn next(&self, state: usize, term: usize) -> usize {
        if state == 0 {
            self.root[term]
        } else {
            *self
                .edges
                .get(&((state as u64) << 32 | term as u64))
                .unwrap_or(&0)
        }
    }
    fn build(queries: &[Query], terms: usize) -> Self {
        let mut a = Self {
            nodes: vec![Node::default()],
            edges: Table::default(),
            root: vec![0; terms],
            next_query: vec![0; queries.iter().filter(|q| q.phrase.is_some()).count()],
        };
        let mut children: Vec<Vec<(usize, usize)>> = vec![Vec::new()];
        for query in queries {
            if let Some(id) = query.phrase {
                let mut state = 0;
                for &term in &query.terms {
                    let mut next = a.next(state, term);
                    if next == 0 {
                        next = a.nodes.len();
                        a.nodes.push(Node::default());
                        children.push(Vec::new());
                        children[state].push((term, next));
                        if state == 0 {
                            a.root[term] = next;
                        } else {
                            a.edges.insert((state as u64) << 32 | term as u64, next);
                        }
                    }
                    state = next;
                }
                a.next_query[id] = a.nodes[state].terminal;
                a.nodes[state].terminal = id as u32 + 1;
            }
        }
        let mut queue: VecDeque<usize> = children[0].iter().map(|&(_, id)| id).collect();
        while let Some(state) = queue.pop_front() {
            for &(term, id) in &children[state] {
                let mut fail = a.nodes[state].fail as usize;
                while fail != 0 && a.next(fail, term) == 0 {
                    fail = a.nodes[fail].fail as usize;
                }
                fail = a.next(fail, term);
                a.nodes[id].fail = fail as u32;
                a.nodes[id].output = if a.nodes[fail].terminal == 0 {
                    a.nodes[fail].output
                } else {
                    fail as u32
                };
                queue.push_back(id);
            }
        }
        a
    }
}

#[derive(Clone, Default)]
struct Summary {
    count: u32,
    sum: u64,
    first: Vec<u32>,
}
impl Summary {
    fn add(&mut self, id: u32) {
        self.count += 1;
        self.sum += u64::from(id);
        if self.first.len() < 10 {
            self.first.push(id);
        }
    }
    fn merge(&mut self, other: &Self) {
        self.count += other.count;
        self.sum += other.sum;
        self.first
            .extend(other.first.iter().take(10 - self.first.len()));
    }
    fn write(&self, output: &mut Vec<u8>) {
        write!(output, "{} {}", self.count, self.sum).unwrap();
        for id in &self.first {
            write!(output, " {id}").unwrap();
        }
        output.push(b'\n');
    }
}
struct Parsed {
    postings: Vec<Set>,
    base: u32,
    phrases: Vec<Summary>,
    documents: usize,
}
fn parse(
    input: &[u8],
    dictionary: &Table<[u8; 32], usize>,
    needed: &[bool],
    a: &Automaton,
    phrases: usize,
) -> Parsed {
    let doc_id = |bytes: &[u8]| {
        bytes
            .iter()
            .take_while(|&&b| b != b'\t')
            .fold(0u32, |n, &b| n * 10 + u32::from(b - b'0'))
    };
    let base = if input.is_empty() { 0 } else { doc_id(input) };
    let documents = if input.is_empty() {
        0
    } else {
        let last = input[..input.len() - 1]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p + 1);
        doc_id(&input[last..]) as usize + 1
    };
    let local_documents = documents - base as usize;
    let mut out = Parsed {
        postings: vec![Set::Sparse(Vec::new()); dictionary.len()],
        base,
        phrases: vec![Summary::default(); phrases],
        documents: 0,
    };
    let mut last = vec![u32::MAX; phrases];
    let mut pos = 0;
    let mut token = [0u8; 32];
    while pos < input.len() {
        let mut doc = 0u32;
        while input[pos] != b'\t' {
            doc = doc * 10 + u32::from(input[pos] - b'0');
            pos += 1;
        }
        pos += 1;
        out.documents = doc as usize + 1;
        let mut state = 0;
        while input[pos] != b'\n' {
            token.fill(0);
            let mut len = 0;
            while input[pos].is_ascii_alphanumeric() {
                token[len] = input[pos].to_ascii_lowercase();
                len += 1;
                pos += 1;
            }
            if len == 0 {
                pos += 1;
                continue;
            }
            if let Some(&term) = dictionary.get(&token) {
                if needed[term] {
                    out.postings[term].insert(doc, base, local_documents);
                }
                let mut next = a.next(state, term);
                while state != 0 && next == 0 {
                    state = a.nodes[state].fail as usize;
                    next = a.next(state, term);
                }
                state = next;
                let mut found = state;
                while found != 0 {
                    let mut terminal = a.nodes[found].terminal;
                    while terminal != 0 {
                        let query = terminal as usize - 1;
                        if last[query] != doc {
                            last[query] = doc;
                            out.phrases[query].add(doc);
                        }
                        terminal = a.next_query[query];
                    }
                    found = a.nodes[found].output as usize;
                }
            } else {
                state = 0;
            }
        }
        pos += 1;
    }
    out
}

#[derive(Clone)]
enum Set {
    Sparse(Vec<u32>),
    Dense(Vec<u64>),
}
impl Set {
    fn insert(&mut self, doc: u32, base: u32, documents: usize) {
        match self {
            Self::Sparse(ids) => {
                if ids.last() != Some(&doc) {
                    ids.push(doc);
                }
                if ids.len() > (documents / 32).max(64) {
                    let mut bits = vec![0; documents.div_ceil(64)];
                    for &id in ids.iter() {
                        let id = (id - base) as usize;
                        bits[id / 64] |= 1 << (id % 64);
                    }
                    *self = Self::Dense(bits);
                }
            }
            Self::Dense(bits) => {
                let id = (doc - base) as usize;
                bits[id / 64] |= 1 << (id % 64);
            }
        }
    }
    fn visit(&self, base: u32, mut f: impl FnMut(u32)) {
        match self {
            Self::Sparse(ids) => {
                for &id in ids {
                    f(id);
                }
            }
            Self::Dense(words) => {
                for (i, &word) in words.iter().enumerate() {
                    let mut bits = word;
                    while bits != 0 {
                        f(base + (i * 64 + bits.trailing_zeros() as usize) as u32);
                        bits &= bits - 1;
                    }
                }
            }
        }
    }
    fn count(&self) -> usize {
        match self {
            Self::Sparse(ids) => ids.len(),
            Self::Dense(bits) => bits.iter().map(|n| n.count_ones() as usize).sum(),
        }
    }

    fn contains(&self, id: u32) -> bool {
        match self {
            Self::Sparse(ids) => ids.binary_search(&id).is_ok(),
            Self::Dense(bits) => bits[id as usize / 64] & (1 << (id % 64)) != 0,
        }
    }
    fn combine(&mut self, rhs: &Self, op: u8, documents: usize) {
        match (&mut *self, rhs) {
            (Self::Dense(left), Self::Dense(right)) => {
                for (a, b) in left.iter_mut().zip(right) {
                    *a = match op {
                        b'A' => *a & b,
                        b'O' => *a | b,
                        _ => *a & !b,
                    };
                }
            }
            (Self::Dense(left), Self::Sparse(right)) if op != b'A' => {
                for &id in right {
                    let bit = 1 << (id % 64);
                    if op == b'O' {
                        left[id as usize / 64] |= bit;
                    } else {
                        left[id as usize / 64] &= !bit;
                    }
                }
            }
            (Self::Dense(_), Self::Sparse(right)) => {
                *self = Self::Sparse(
                    right
                        .iter()
                        .copied()
                        .filter(|&id| self.contains(id))
                        .collect(),
                );
            }
            (Self::Sparse(left), _) if op != b'O' => {
                left.retain(|&id| rhs.contains(id) == (op == b'A'));
            }
            (Self::Sparse(left), Self::Dense(right)) => {
                let mut bits = right.clone();
                for &id in left.iter() {
                    bits[id as usize / 64] |= 1 << (id % 64);
                }
                *self = Self::Dense(bits);
            }
            (Self::Sparse(left), Self::Sparse(right)) => {
                let mut merged = Vec::with_capacity(left.len() + right.len());
                let (mut x, mut y) = (0, 0);
                while x < left.len() && y < right.len() {
                    let a = left[x];
                    let b = right[y];
                    merged.push(a.min(b));
                    x += usize::from(a <= b);
                    y += usize::from(b <= a);
                }
                merged.extend_from_slice(&left[x..]);
                merged.extend_from_slice(&right[y..]);
                *self = Self::from_sorted(merged, documents);
            }
        }
    }
    fn from_sorted(ids: Vec<u32>, documents: usize) -> Self {
        if ids.len() <= documents / 32 {
            return Self::Sparse(ids);
        }
        let mut bits = vec![0; documents.div_ceil(64)];
        for id in ids {
            bits[id as usize / 64] |= 1 << (id % 64);
        }
        Self::Dense(bits)
    }
    fn summary(&self) -> Summary {
        let mut summary = Summary::default();
        match self {
            Self::Sparse(ids) => {
                for &id in ids {
                    summary.add(id);
                }
            }
            Self::Dense(bits) => {
                for (word, &bits) in bits.iter().enumerate() {
                    let mut bits = bits;
                    while bits != 0 {
                        summary.add((word * 64 + bits.trailing_zeros() as usize) as u32);
                        bits &= bits - 1;
                    }
                }
            }
        }
        summary
    }
}

fn solve(data: &Path, out: &Path) -> io::Result<()> {
    let query_text = fs::read(data.join("queries.txt"))?;
    let mut dictionary: Table<[u8; 32], usize> = Table::default();
    let mut needed = Vec::new();
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
            let id = match dictionary.get(&key) {
                Some(&id) => id,
                None => {
                    let id = dictionary.len();
                    dictionary.insert(key, id);
                    needed.push(false);
                    id
                }
            };
            query.terms.push(id);
            needed[id] |= !phrase;
        }
        if phrase {
            query.phrase = Some(phrases);
            phrases += 1;
        }
        queries.push(query);
    }
    let automaton = Automaton::build(&queries, dictionary.len());
    let mapping = Mapping::open(&data.join("docs.txt"))?;
    let input = mapping.bytes();
    let workers = thread::available_parallelism()
        .map_or(1, usize::from)
        .min(12)
        .min(input.len().div_ceil(64 * 1024).max(1));
    let mut parsed = thread::scope(|scope| {
        let mut jobs = Vec::new();
        let mut start = 0;
        for worker in 0..workers {
            let mut end = input.len() * (worker + 1) / workers;
            while end < input.len() && end > 0 && input[end - 1] != b'\n' {
                end += 1;
            }
            let chunk = &input[start..end];
            let dictionary = &dictionary;
            let needed = &needed;
            let automaton = &automaton;
            jobs.push(scope.spawn(move || parse(chunk, dictionary, needed, automaton, phrases)));
            start = end;
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    let documents = parsed.iter().map(|p| p.documents).max().unwrap_or(0);
    let mut phrase_results = vec![Summary::default(); phrases];
    for part in &parsed {
        for (result, summary) in phrase_results.iter_mut().zip(&part.phrases) {
            result.merge(summary);
        }
    }
    let mut index = Vec::with_capacity(dictionary.len());
    for term in 0..dictionary.len() {
        let total: usize = parsed.iter().map(|part| part.postings[term].count()).sum();
        let mut merged = if total > documents / 32 {
            Set::Dense(vec![0; documents.div_ceil(64)])
        } else {
            Set::Sparse(Vec::with_capacity(total))
        };
        for part in &mut parsed {
            let postings = std::mem::replace(&mut part.postings[term], Set::Sparse(Vec::new()));
            match &mut merged {
                Set::Sparse(ids) => postings.visit(part.base, |id| ids.push(id)),
                Set::Dense(bits) => {
                    postings.visit(part.base, |id| bits[id as usize / 64] |= 1 << (id % 64))
                }
            }
        }
        index.push(merged);
    }
    drop(parsed);
    let results = thread::scope(|scope| {
        let mut jobs = Vec::new();
        for chunk in queries.chunks(queries.len().div_ceil(workers).max(1)) {
            let index = &index;
            let phrase_results = &phrase_results;
            jobs.push(scope.spawn(move || {
                let mut output = Vec::new();
                for query in chunk {
                    if let Some(id) = query.phrase {
                        phrase_results[id].write(&mut output);
                    } else {
                        let mut result = index[query.terms[0]].clone();
                        for (&op, &term) in query.ops.iter().zip(&query.terms[1..]) {
                            result.combine(&index[term], op, documents);
                        }
                        result.summary().write(&mut output);
                    }
                }
                output
            }));
        }
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<Vec<_>>()
    });
    fs::create_dir_all(out)?;
    let mut output = File::create(out.join("results.txt"))?;
    for bytes in results {
        output.write_all(&bytes)?;
    }
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
