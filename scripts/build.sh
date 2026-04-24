#!/usr/bin/env bash
# Build the uncalibrated-sextant UEFI binary inside Docker.
set -euo pipefail

IMAGE_TAG="uncalibrated-sextant-build:1.88.0"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Build the image if it doesn't exist. Cheap to re-run; docker
# build is a no-op if the layer cache is already warm.
docker build -t "$IMAGE_TAG" "$REPO_ROOT"

# Run cargo build inside the container. Mount the source in, mount
# a named volume for target/ so host file ownership stays clean
# and incremental builds are preserved across invocations.
docker run --rm \
    -v "$REPO_ROOT":/work \
    -v uncalibrated-sextant-target:/work/target \
    -w /work \
    "$IMAGE_TAG" \
    cargo build --release
