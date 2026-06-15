#!/usr/bin/env bash
# Build the uncalibrated-sextant UEFI binary inside Docker.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/rust-docker.sh
source "$REPO_ROOT/scripts/rust-docker.sh"

# Build the image if needed, then make the named volumes writable by the host
# user (see scripts/rust-docker.sh) so the build runs as us, not root.
ensure_image
rust_docker_prepare

run_in_docker cargo build --release
