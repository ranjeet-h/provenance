#!/usr/bin/env bash
set -euo pipefail
export PATH="$HOME/Library/pnpm/bin:$PATH"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "== frontend: typecheck =="
pnpm typecheck

echo "== frontend: lint =="
pnpm lint

echo "== frontend: test =="
pnpm test

echo "== frontend: build =="
pnpm build

echo "== rust: fmt =="
cargo fmt --check

echo "== rust: clippy =="
cargo clippy --workspace --all-targets --all-features -- -D warnings

echo "== rust: test =="
cargo test --workspace

echo "ALL CHECKS PASSED"
