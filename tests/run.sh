#!/usr/bin/env bash
# Forge test runner.
#
# Runs the program build, the Rust workspace tests (including Golden vectors and
# LiteSVM program tests), and the e2e placeholder.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> anchor build"
anchor build

echo "==> cargo test --workspace"
cargo test --workspace

echo "==> e2e"
bash tests/e2e/run.sh

echo "==> OK"
