#!/usr/bin/env bash
# Run rustfmt and clippy inside the build container.
# Used by pre-commit; can also be invoked manually:
#   ./scripts/check-rust.sh check   (default)
#   ./scripts/check-rust.sh fix
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/rust-docker.sh
source "$REPO_ROOT/scripts/rust-docker.sh"
MODE="${1:-check}"

ensure_image
rust_docker_prepare

FAILED=0

echo "=== uncalibrated-sextant: rustfmt ==="
if [ "$MODE" = "fix" ]; then
    run_in_docker cargo fmt --all || FAILED=1
else
    run_in_docker cargo fmt --all -- --check || FAILED=1
fi

echo "=== uncalibrated-sextant: clippy ==="
if [ "$MODE" = "fix" ]; then
    run_in_docker cargo clippy --bins --fix \
        --allow-dirty --allow-staged -- -D warnings || FAILED=1
else
    run_in_docker cargo clippy --bins -- -D warnings || FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
    echo "Some checks failed."
    exit 1
fi

echo "All checks passed."
