use std::{
    fs::{self, File},
    io::{Result, Write},
    path::Path,
};

pub struct StartupScripts;

impl StartupScripts {
    pub fn write_script_to_file(script: &str, script_path: &Path) -> Result<()> {
        if script_path.exists() {
            // If script already exists, delete it.
            // prevents versioning issues.
            fs::remove_file(script_path)?;
        }
        let mut file = File::create(script_path)?;
        file.write_all(script.as_bytes())?;
        Ok(())
    }

    pub fn bootstrap() -> &'static str {
        r#"
#!/bin/bash
set -e

# start faucet
nohup solana-faucet --keypair bootstrap-accounts/faucet.json &

# Start the bootstrap validator node
# shellcheck disable=SC1091
source /home/solana/k8s-cluster-scripts/common.sh

program="agave-validator"

no_restart=0

echo "PROGRAM: $program"

args=()
while [[ -n $1 ]]; do
  if [[ ${1:0:1} = - ]]; then
    if [[ $1 = --init-complete-file ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --gossip-host ]]; then # set with env variables
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --gossip-port ]]; then # set with env variables
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --dev-halt-at-slot ]]; then # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --dynamic-port-range ]]; then # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --limit-ledger-size ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --no-rocksdb-compaction ]]; then # not enabled in net.sh
      args+=("$1")
      shift
    elif [[ $1 = --enable-rpc-transaction-history ]]; then # enabled through full-rpc
      args+=("$1")
      shift
    elif [[ $1 = --rpc-pubsub-enable-block-subscription ]]; then # not enabled in net.sh
      args+=("$1")
      shift
    elif [[ $1 = --enable-cpi-and-log-storage ]]; then # not enabled in net.sh
      args+=("$1")
      shift
    elif [[ $1 = --enable-extended-tx-metadata-storage ]]; then # enabled through full-rpc
      args+=("$1")
      shift
    elif [[ $1 = --enable-rpc-bigtable-ledger-storage ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --tpu-disable-quic ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --tpu-enable-udp ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --rpc-send-batch-ms ]]; then # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --rpc-send-batch-size ]]; then # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --skip-poh-verify ]]; then 
      args+=("$1")
      shift
    elif [[ $1 = --no-restart ]]; then # not enabled in net.sh
      no_restart=1
      shift
    elif [[ $1 == --wait-for-supermajority ]]; then 
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --expected-bank-hash ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --accounts ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --maximum-snapshots-to-retain ]]; then  # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --no-snapshot-fetch ]]; then 
      args+=("$1")
      shift
    elif [[ $1 == --accounts-db-skip-shrink ]]; then
      args+=("$1")
      shift
    elif [[ $1 == --require-tower ]]; then 
      args+=("$1")
      shift
    elif [[ $1 = --log-messages-bytes-limit ]]; then # not enabled in net.sh
      args+=("$1" "$2")
      shift 2
    else
      echo "Unknown argument: $1"
      $program --help
      exit 1
    fi
  else
    echo "Unknown argument: $1"
    $program --help
    exit 1
  fi
done

# These keypairs are created by ./setup.sh and included in the genesis config
identity=bootstrap-accounts/identity.json
vote_account=bootstrap-accounts/vote.json

ledger_dir=/home/solana/ledger
[[ -d "$ledger_dir" ]] || {
  echo "$ledger_dir does not exist"
  exit 1
}

args+=(
  --no-os-network-limits-test \
  --no-wait-for-vote-to-start-leader \
  --snapshot-interval-slots 200 \
  --identity "$identity" \
  --vote-account "$vote_account" \
  --ledger ledger \
  --log - \
  --gossip-host "$MY_POD_IP" \
  --gossip-port 8001 \
  --rpc-port 8899 \
  --rpc-faucet-address "$MY_POD_IP":9900 \
  --no-poh-speed-test \
  --no-incremental-snapshots \
  --full-rpc-api \
  --allow-private-addr \
  --enable-rpc-transaction-history
)

echo "Bootstrap Args"
for arg in "${args[@]}"; do
  echo "$arg"
done

pid=
kill_node() {
  # Note: do not echo anything from this function to ensure $pid is actually
  # killed when stdout/stderr are redirected
  set +ex
  if [[ -n $pid ]]; then
    declare _pid=$pid
    pid=
    kill "$_pid" || true
    wait "$_pid" || true
  fi
}

kill_node_and_exit() {
  kill_node
  exit
}

trap 'kill_node_and_exit' INT TERM ERR

while true; do
  echo "$program ${args[*]}"
  $program "${args[@]}" &
  pid=$!
  echo "pid: $pid"

  if ((no_restart)); then
    wait "$pid"
    exit $?
  fi

  while true; do
    if [[ -z $pid ]] || ! kill -0 "$pid"; then
      echo "\############## validator exited, restarting ##############"
      break
    fi
    sleep 1
  done

  kill_node
done
        "#
    }

    pub fn common() -> &'static str {
        r#"
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
        "#
    }
}
