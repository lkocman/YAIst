#!/bin/bash
# Builds YAIST inside a rootless openSUSE container, for machines without a
# Rust toolchain. Arguments go to cargo (default: build). The binaries land
# in target/ as usual and run on the distribution the image is based on
# (Leap 16.0 by default; edit scripts/build.Containerfile for another).
set -euo pipefail
cd "$(dirname "$0")/.."
IMAGE=localhost/yaist-build:leap16
CARGO_CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/yaist/cargo-home"
mkdir -p "$CARGO_CACHE"
if ! podman image exists "$IMAGE"; then
    podman build -t "$IMAGE" -f scripts/build.Containerfile scripts
fi
[ $# -gt 0 ] || set -- build
exec podman run --rm \
    --userns=keep-id \
    -v "$PWD":/src:Z -v "$CARGO_CACHE":/cargo:Z \
    -e CARGO_HOME=/cargo -w /src \
    "$IMAGE" cargo "$@"
