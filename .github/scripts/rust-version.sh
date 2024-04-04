#!/usr/bin/env bash

# This file maintains the rust versions for use by CI.
#
# Obtain the environment variables without any automatic toolchain updating:
#   $ source <path>/rust-version.sh
#
# Obtain the environment variables updating both stable and nightly, only stable, or
# only nightly:
#   $ source <path>/rust-version.sh all
#   $ source <path>/rust-version.sh stable
#   $ source <path>/rust-version.sh nightly

# Then to build with either stable or nightly:
#   $ cargo +"$rust_stable" build
#   $ cargo +"$rust_nightly" build

repo_root=$(git rev-parse --show-toplevel)

# stable version
if [[ -n $RUST_STABLE_VERSION ]]; then
  stable_version="$RUST_STABLE_VERSION"
else
  # shellcheck disable=SC1090,SC1091
  source "$repo_root/.github/scripts/read-cargo-variable.sh"
  stable_version=$(readCargoVariable channel "$repo_root/rust-toolchain.toml")
fi

# nightly version
if [[ -n $RUST_NIGHTLY_VERSION ]]; then
  nightly_version="$RUST_NIGHTLY_VERSION"
else
  nightly_version=2024-01-05
fi

export rust_stable="$stable_version"
export rust_nightly=nightly-"$nightly_version"

if [ $# -ne 1 ]; then
  echo "Usage: $0 [all|stable|nightly]"
  exit 1
fi

case $1 in
all)
  toolchains=("$rust_stable" "$rust_nightly")
  ;;
stable)
  toolchains=("$rust_stable")
  ;;
nightly)
  toolchains=("$rust_nightly")
  ;;
*)
  echo "Usage: $0 [all|stable|nightly]"
  exit 1
  ;;
esac

for toolchain in "${toolchains[@]}"; do
  if ! cargo +"$toolchain" -V >/dev/null; then
    echo "$0: installing $toolchain"
    rustup install "$toolchain"
    cargo +"$toolchain" -V
  else
    echo "$0: $toolchain has already installed."
  fi
done
