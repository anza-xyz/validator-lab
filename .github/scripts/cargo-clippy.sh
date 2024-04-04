#!/usr/bin/env bash

set -e

repo_root=$(git rev-parse --show-toplevel)

# shellcheck disable=SC1091
source "$repo_root/.github/scripts/rust-version.sh" stable >/dev/null

rustup component add clippy --toolchain="$rust_stable"

cargo +"$rust_stable" clippy
