use std::collections::BTreeMap;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Group {
    latencies: Vec<u32>,
    errors: u64,
}

fn percentiles(values: &mut [u32]) -> [u32; 3] {
    let count = values.len();
    let mut result = [0; 3];
    let mut offset = 0;
    let mut remaining = values;
    for (slot, percentile) in result.iter_mut().zip([50, 95, 99]) {
        let rank = (percentile * count).div_ceil(100) - 1;
        remaining.select_nth_unstable(rank - offset);
        // Keep the pivot: small groups can have identical percentile ranks.
        remaining = &mut remaining[rank - offset..];
        *slot = remaining[0];
        offset = rank;
    }
    result
}

fn summarize(data: &Path, out: &Path) -> io::Result<()> {
    let mut input = BufReader::with_capacity(256 * 1024, File::open(data.join("requests.log"))?);
    let mut hours: BTreeMap<[u8; 13], HashMap<Vec<u8>, Group>> = BTreeMap::new();
    let mut line = Vec::with_capacity(176);
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        // The spec guarantees ASCII, a 27-byte UTC timestamp and single spaces.
        let hour: [u8; 13] = line[..13].try_into().unwrap();
        let mut fields = line[28..line.len() - 1].split(|&byte| byte == b' ');
        fields.next(); // Method does not affect aggregation.
        let endpoint = fields.next().unwrap();
        let status = fields.next().unwrap();
        let latency = fields
            .next()
            .unwrap()
            .iter()
            .fold(0u32, |value, &digit| value * 10 + u32::from(digit - b'0'));
        let endpoints = hours.entry(hour).or_default();
        // Allocate an endpoint key only when this group is first seen.
        if let Some(group) = endpoints.get_mut(endpoint) {
            group.latencies.push(latency);
            group.errors += u64::from(status[0] == b'5');
        } else {
            endpoints.insert(
                endpoint.to_vec(),
                Group {
                    latencies: vec![latency],
                    errors: u64::from(status[0] == b'5'),
                },
            );
        }
    }

    fs::create_dir_all(out)?;
    let mut output = BufWriter::with_capacity(256 * 1024, File::create(out.join("summary.txt"))?);
    for (hour, endpoints) in hours {
        let mut endpoints: Vec<_> = endpoints.into_iter().collect();
        endpoints.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        for (endpoint, mut group) in endpoints {
            let count = group.latencies.len() as u64;
            let [p50, p95, p99] = percentiles(&mut group.latencies);
            let ppm = group.errors * 1_000_000 / count;
            output.write_all(&hour)?;
            output.write_all(b":00:00Z ")?;
            output.write_all(&endpoint)?;
            writeln!(output, " {count} {p50} {p95} {p99} {} {ppm}", group.errors)?;
        }
    }
    output.flush()
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
