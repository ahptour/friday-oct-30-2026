use std::fs;
use std::io;
use std::path::Path;

fn source_lines(path: &Path) -> io::Result<usize> {
    if path.is_dir() {
        return fs::read_dir(path)?
            .try_fold(0, |total, entry| Ok(total + source_lines(&entry?.path())?));
    }
    Ok(fs::read_to_string(path)?
        .lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty()
                && !line.starts_with("//")
                && (path.extension().is_some_and(|extension| extension == "rs")
                    || !line.starts_with('#'))
        })
        .count())
}

fn main() -> io::Result<()> {
    let mut total = 0;
    // The byte-identical published Python oracles are exempt from source scoring.
    for path in ["src", "build.rs", "Cargo.toml", "Dockerfile"] {
        println!("cargo:rerun-if-changed={path}");
        total += source_lines(Path::new(path))?;
    }
    println!("cargo:rustc-env=STATS_SOURCE_LINES={total}");
    println!("cargo:rerun-if-changed=.speed-demon.toml");
    let manifest = fs::read_to_string(".speed-demon.toml")?;
    let dependencies = manifest
        .lines()
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == "dependency_count")
        .expect("dependency_count is missing from .speed-demon.toml")
        .1
        .split('#')
        .next()
        .unwrap()
        .trim()
        .parse::<usize>()
        .expect("dependency_count must be a nonnegative integer");
    println!(
        "cargo:rustc-env=STATS_DEPENDENCY_WEIGHT={}",
        1 + dependencies
    );
    Ok(())
}
