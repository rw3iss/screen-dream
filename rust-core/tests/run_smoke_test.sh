#!/usr/bin/env bash
# Build the FFI crate and compile + run the C smoke test.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building FFI crate..."
cargo build -p ffi

echo "==> Compiling C smoke test..."
gcc tests/smoke_test.c -o tests/smoke_test \
    -L target/debug \
    -lscreen_dream_ffi \
    -Icrates/ffi

echo "==> Running smoke test..."
LD_LIBRARY_PATH=target/debug ./tests/smoke_test
