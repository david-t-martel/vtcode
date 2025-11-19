#!/usr/bin/env bash
# Collect rust-analyzer diagnostics for the workspace (via cargo check json) and write to /tmp/check.json
set -euo pipefail
RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-stable}
OUT=${1:-/tmp/check.json}
RUSTUP_TOOLCHAIN=$RUSTUP_TOOLCHAIN cargo check --message-format=json | tee "$OUT"
echo "wrote diagnostics to $OUT" >&2
