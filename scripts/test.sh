#!/usr/bin/env bash
# Run host-side unit tests for uncalibrated-sextant inside Docker.
#
# The default cargo target is `x86_64-unknown-uefi` (see
# `.cargo/config.toml`), which has no test harness. This script
# overrides the target to `x86_64-unknown-linux-gnu` so the test
# profile picks up `std` and libtest. Only the pure modules
# (`digest`, `event`) compile under the test profile; the
# UEFI-dependent modules in `main.rs` are gated behind
# `#[cfg(not(test))]`.
set -euo pipefail

IMAGE_TAG="uncalibrated-sextant-build:1.88.0"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build the image if it doesn't exist. Cheap to re-run.
docker build -t "$IMAGE_TAG" "$REPO_ROOT"

docker run --rm \
    -v "$REPO_ROOT":/work \
    -v uncalibrated-sextant-target:/work/target \
    -w /work \
    "$IMAGE_TAG" \
    cargo test --target x86_64-unknown-linux-gnu "$@"
