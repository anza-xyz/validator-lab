#!/usr/bin/env bash

set -e

repo_root=$(git rev-parse --show-toplevel)

# shellcheck disable=SC1091
source "$repo_root/.github/scripts/rust-version.sh" stable >/dev/null

cargo_audit_ignores=(
  # ed25519-dalek
  --ignore RUSTSEC-2022-0093
)

cargo +"$rust_stable" audit "${cargo_audit_ignores[@]}"
