#!/bin/sh
set -eu

target=$(rustc -vV | sed -n 's/^host: //p')
profdata="$(rustc --print sysroot)/lib/rustlib/$target/bin/llvm-profdata"
profiles=/tmp/pgo-profiles
mkdir -p "$profiles"
export CARGO_TARGET_DIR=/tmp/pgo-target
export RUSTFLAGS="-C target-cpu=native -C profile-generate=$profiles"
cargo build --release --offline --locked --target "$target"
for seed in 7 19 103; do
    LLVM_PROFILE_FILE="$profiles/seed-$seed-%m-%p.profraw" \
        "$CARGO_TARGET_DIR/$target/release/solution" \
        --data "/training/$seed" --out /tmp/training-output
done
"$profdata" merge -o /tmp/merged.profdata "$profiles"/*.profraw
export RUSTFLAGS="-C target-cpu=native -C profile-use=/tmp/merged.profdata -C llvm-args=-pgo-warn-missing-function"
cargo build --release --offline --locked --target "$target"
mkdir -p /src/target/release
cp "$CARGO_TARGET_DIR/$target/release/solution" /src/target/release/solution
