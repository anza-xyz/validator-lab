#!/usr/bin/env bash

set -e

repo_root=$(git rev-parse --show-toplevel)

# shellcheck disable=SC1091
source "$repo_root/.github/scripts/rust-version.sh" nightly >/dev/null

rustup component add rustfmt --toolchain="$rust_nightly"
cargo +"$rust_nightly" fmt --all
