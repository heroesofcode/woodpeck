#!/usr/bin/env bash
# Builds the browser version into web/dist. Needs the wasm32-unknown-unknown target, a C
# compiler that targets wasm32 for the bundled SQLite (clang by default; set
# CC_wasm32_unknown_unknown to override), and wasm-bindgen-cli matching Cargo.lock.
set -euo pipefail
cd "$(dirname "$0")/.."
export CC_wasm32_unknown_unknown="${CC_wasm32_unknown_unknown:-clang}"
target_dir="${CARGO_TARGET_DIR:-target}"
cargo build --release --locked --target wasm32-unknown-unknown --no-default-features --bin woodpeck
rm -rf web/dist
wasm-bindgen --target web --no-typescript --out-dir web/dist \
  "$target_dir/wasm32-unknown-unknown/release/woodpeck.wasm"
cp web/index.html web/dist/
