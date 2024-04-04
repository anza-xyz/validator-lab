#!/usr/bin/env bash

set -e

repo_root=$(git rev-parse --show-toplevel)

# shellcheck disable=SC1091
source "$repo_root/.github/scripts/rust-version.sh" stable >/dev/null

cargo +"$rust_stable" clippy
