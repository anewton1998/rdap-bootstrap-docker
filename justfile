# Local development helpers for rdap-bootstrap-docker.
#
# The published image is built by CI, which resolves the live icann-rdap `dev`
# SHA and passes it as ICANN_RDAP_REF. These recipes mirror that so local builds
# also track `dev` while keeping the Docker layer / cargo compile cache warm.

repo  := "https://github.com/icann/icann-rdap.git"
image := "rdap-bootstrap"

# Show available recipes.
default:
    @just --list

# Build the image from the latest icann-rdap dev HEAD (mirrors CI: resolves the dev SHA).
build:
    #!/usr/bin/env bash
    set -euo pipefail
    sha="$(git ls-remote {{repo}} dev | cut -f1)"
    [ -n "$sha" ] || { echo "could not resolve icann-rdap dev SHA" >&2; exit 1; }
    echo "building {{image}} from icann-rdap dev @ $sha"
    docker build --build-arg ICANN_RDAP_REF="$sha" -t {{image}} .

# Run the locally-built image (binds 3000).
run:
    docker run --rm -p 3000:3000 {{image}}

# Full clean rebuild (slow: discards layer + cargo cache-mount reuse).
build-nocache:
    #!/usr/bin/env bash
    set -euo pipefail
    sha="$(git ls-remote {{repo}} dev | cut -f1)"
    [ -n "$sha" ] || { echo "could not resolve icann-rdap dev SHA" >&2; exit 1; }
    docker build --no-cache --build-arg ICANN_RDAP_REF="$sha" -t {{image}} .

# Regenerate the synthesized redirect data into ./data (then commit it).
gen-data:
    cargo run --release --manifest-path tools/rdap-tld-redirect-gen/Cargo.toml -- --data-dir ./data
