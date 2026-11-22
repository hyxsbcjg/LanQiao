#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd "$(dirname "$0")" && pwd)"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HOME/lanqiao-target-r1}
export CARGO_PROFILE_RELEASE_LTO=${CARGO_PROFILE_RELEASE_LTO:-fat}
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}
if (( $# > 1 )); then
    echo "用法：$0 [输出目录]" >&2
    exit 2
fi
out_dir=${1:-$HOME/r1-ref/pkg}
cargo build --manifest-path "$script_dir/Cargo.toml" --release --target wasm32-unknown-unknown -p lanqiao-web
wasm-bindgen "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/lanqiao_web.wasm" \
    --target web --out-dir "$out_dir"
if command -v wasm-opt >/dev/null 2>&1; then
    wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int \
        "$out_dir/lanqiao_web_bg.wasm" -o "$out_dir/lanqiao_web_bg.wasm"
fi
