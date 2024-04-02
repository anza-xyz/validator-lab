# |source| this file
#
# Common utilities shared by other scripts in this directory
#
# The following directive disable complaints about unused variables in this
# file:
# shellcheck disable=2034

prebuild=
if [[ $1 = "--prebuild" ]]; then
  prebuild=true
fi

if [[ $(uname) != Linux ]]; then
  # Protect against unsupported configurations to prevent non-obvious errors
  # later. Arguably these should be fatal errors but for now prefer tolerance.
  if [[ -n $SOLANA_CUDA ]]; then
    echo "Warning: CUDA is not supported on $(uname)"
    SOLANA_CUDA=
  fi
fi

if [[ -n $USE_INSTALL || ! -f "$SOLANA_ROOT"/Cargo.toml ]]; then
  # echo "define if solana program"
  solana_program() {
    # echo "call if solana program"
    declare program="$1"
    if [[ -z $program ]]; then
      printf "solana"
    else
      printf "solana-%s" "$program"
    fi
  }
else
  echo "define else solana program"
  solana_program() {
    echo "call if solana program"
    declare program="$1"
    declare crate="$program"
    if [[ -z $program ]]; then
      crate="cli"
      program="solana"
    else
      program="solana-$program"
    fi

    if [[ -n $NDEBUG ]]; then
      maybe_release=--release
    fi

    # Prebuild binaries so that CI sanity check timeout doesn't include build time
    if [[ $prebuild ]]; then
      (
        set -x
        # shellcheck disable=SC2086 # Don't want to double quote
        cargo $CARGO_TOOLCHAIN build $maybe_release --bin $program
      )
    fi

    printf "cargo $CARGO_TOOLCHAIN run $maybe_release  --bin %s %s -- " "$program"
  }
fi

solana_bench_tps=$(solana_program bench-tps)
solana_faucet=$(solana_program faucet)
solana_validator=$(solana_program validator)
solana_validator_cuda="$solana_validator --cuda"
solana_genesis=$(solana_program genesis)
solana_gossip=$(solana_program gossip)
solana_keygen=$(solana_program keygen)
solana_ledger_tool=$(solana_program ledger-tool)
solana_cli=$(solana_program)

export RUST_BACKTRACE=1

# https://gist.github.com/cdown/1163649
urlencode() {
  declare s="$1"
  declare l=$((${#s} - 1))
  for i in $(seq 0 $l); do
    declare c="${s:$i:1}"
    case $c in
      [a-zA-Z0-9.~_-])
        echo -n "$c"
        ;;
      *)
        printf '%%%02X' "'$c"
        ;;
    esac
  done
}

default_arg() {
  declare name=$1
  declare value=$2

  for arg in "${args[@]}"; do
    if [[ $arg = "$name" ]]; then
      return
    fi
  done

  if [[ -n $value ]]; then
    args+=("$name" "$value")
  else
    args+=("$name")
  fi
}

replace_arg() {
  declare name=$1
  declare value=$2

  default_arg "$name" "$value"

  declare index=0
  for arg in "${args[@]}"; do
    index=$((index + 1))
    if [[ $arg = "$name" ]]; then
      args[$index]="$value"
    fi
  done
}
