#!/usr/bin/env bash
# Shared helpers for running cargo in the build container as the host user.
# Sourced by build.sh and check-rust.sh.
#
# Running as the host uid keeps files written into the bind-mounted repo
# (notably Cargo.lock) owned by us rather than root. Root-owned files block a
# later host-user run with "Permission denied" and cannot be removed without
# sudo. Build output and the cargo download cache live in named volumes so
# host file ownership stays clean and incremental state persists; Docker
# creates those volumes owned by root, so we make them writable up front.
#
# The sourcing script must set REPO_ROOT before calling these helpers.

IMAGE_TAG="uncalibrated-sextant-build:1.88.0"
TARGET_VOLUME="uncalibrated-sextant-target"
CARGO_VOLUME="uncalibrated-sextant-cargo"
RUST_UID="$(id -u)"
RUST_GID="$(id -g)"

# Build the image if it is missing.
ensure_image() {
    if ! docker image inspect "$IMAGE_TAG" >/dev/null 2>&1; then
        docker build -t "$IMAGE_TAG" "$REPO_ROOT"
    fi
}

# Make a named volume writable by the host user. Probe writability as the host
# uid and chown the volume once if needed -- no sudo, and cheap on later runs.
_ensure_volume_writable() {
    local vol="$1"
    if ! docker run --rm -u "$RUST_UID:$RUST_GID" -v "$vol":/probe "$IMAGE_TAG" \
            sh -c '[ -w /probe ]' >/dev/null 2>&1; then
        echo "Fixing ownership of the $vol volume ..."
        docker run --rm -v "$vol":/probe alpine \
            chown -R "$RUST_UID:$RUST_GID" /probe
    fi
}

# Prepare the named volumes and heal a Cargo.lock left root-owned by an older
# run, so cargo (and pre-commit's end-of-file-fixer) can write it.
rust_docker_prepare() {
    _ensure_volume_writable "$TARGET_VOLUME"
    _ensure_volume_writable "$CARGO_VOLUME"
    if [ -e "$REPO_ROOT/Cargo.lock" ] && [ ! -w "$REPO_ROOT/Cargo.lock" ]; then
        echo "Fixing ownership of Cargo.lock ..."
        docker run --rm -v "$REPO_ROOT/Cargo.lock":/fixme alpine \
            chown "$RUST_UID:$RUST_GID" /fixme
    fi
}

# Run a cargo command in the build container as the host user. CARGO_HOME lives
# in its own volume (not under target/) so `cargo clean` cannot wipe the cache.
run_in_docker() {
    docker run --rm \
        -u "$RUST_UID:$RUST_GID" \
        -e HOME=/cargo \
        -e CARGO_HOME=/cargo \
        -v "$REPO_ROOT":/work \
        -v "$TARGET_VOLUME":/work/target \
        -v "$CARGO_VOLUME":/cargo \
        -w /work \
        "$IMAGE_TAG" \
        "$@"
}
