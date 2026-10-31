use std::ffi::c_long;
use std::io::{self, Write};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

// rusage on 64-bit Linux and macOS: two 16-byte timevals, then 14 long counters.
// The timeval fields stay opaque; only the following ru_maxrss field is read.
#[repr(C)]
#[derive(Default)]
struct Usage {
    times: [c_long; 4],
    max_rss: c_long,
    counters: [c_long; 13],
}

unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut Usage) -> i32;
}

struct Snapshot {
    started: Instant,
    source_lines: usize,
    dependency_weight: usize,
}

impl Snapshot {
    fn report(&self, state: &str) -> io::Result<()> {
        let elapsed = self.started.elapsed().as_secs_f64();
        let mut usage = Usage::default();
        // RUSAGE_SELF = 0; Usage has the native size/alignment and is writable.
        if unsafe { getrusage(0, &mut usage) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let peak_mb = usage.max_rss as f64
            * if cfg!(target_os = "macos") {
                1.0
            } else {
                1024.0
            }
            / 1_000_000.0;
        let coefficient = (32.05 / elapsed.max(1e-9)).powf(0.50)
            * (228.0 / peak_mb.max(1e-9)).min(50.0).powf(0.20)
            * (56.0 / self.source_lines.max(1) as f64).min(1.0).powf(0.15)
            * (1.0 / self.dependency_weight as f64).min(1.0).powf(0.15);
        writeln!(
            io::stderr(),
            "[stats {state}] T={elapsed:.2} s M={peak_mb:.2} MB S~{} lines D={} K~{coefficient:.3}",
            self.source_lines,
            self.dependency_weight,
        )
    }
}

pub struct Reporter {
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}

impl Reporter {
    pub fn start() -> io::Result<Self> {
        let snapshot = Snapshot {
            started: Instant::now(),
            source_lines: env!("STATS_SOURCE_LINES").parse().unwrap(),
            dependency_weight: env!("STATS_DEPENDENCY_WEIGHT").parse().unwrap(),
        };
        let _ = writeln!(
            io::stderr(),
            "[stats] Local diagnostics: T=elapsed; M=process peak RSS; \
             S=nonblank/noncomment Rust + Cargo.toml + Dockerfile lines before formatting/scc; \
             D=1 + manifest runtime dependencies. Official T/M/S are measured by the harness. \
             K uses these local estimates and is provisional while running.\n\
             [stats] References (weight, ratio cap): T=32.05 s (0.50, none); \
             M=228 MB (0.20, 50x); S=56 lines (0.15, 1x); D=1 (0.15, 1x)."
        );
        let _ = snapshot.report("start");
        let (stop, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("ingest-stats".into())
            .spawn(move || {
                while receiver.recv_timeout(Duration::from_secs(3))
                    == Err(mpsc::RecvTimeoutError::Timeout)
                {
                    let _ = snapshot.report("running");
                }
                let _ = snapshot.report("final");
            })?;
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
