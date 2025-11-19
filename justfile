set shell := ["bash", "-lc"]
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]
export RUSTC_WRAPPER := "sccache"
export CARGO_BUILD_PIPELINING := "true"

# Default help target
default:
    @just --list

# Format the entire workspace
fmt:
    cargo fmt

# Run workspace checks
check:
    cargo check

# Fast dev build for the current host
build:
    cargo build --release

# Windows-native release build
# On Windows, this runs directly in PowerShell.
# On Unix-like hosts, it runs in the configured shell and still just calls cargo.
build-windows:
    cargo build --release --target x86_64-pc-windows-msvc

# Optimized WSL->Windows build: runs cargo inside Windows using a converted path and shared target dir
wsl-windows-build target_dir="C:/vtcode/target-windows":
    set -euxo pipefail
    WIN_PATH=$(wslpath -m "$PWD")
    powershell.exe -NoLogo -Command "cd '${WIN_PATH}'; $env:CARGO_TARGET_DIR='{{target_dir}}'; cargo build --release --target x86_64-pc-windows-msvc"

# Clean artifacts from all targets (shared between Windows + WSL)
clean:
    cargo clean

# Run tests for the entire workspace
test:
    cargo test --workspace

# Run clippy with strict settings across the workspace
clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# Full code-quality check (delegates to scripts/check.sh)
check-all:
    bash scripts/check.sh

# Run Ollama-based autofix helper with optional arguments
autofix-ollama *args:
    bash scripts/ollama-autofix.sh {{args}}

# Show sccache statistics for debugging
sccache-stats:
    sccache --show-stats
