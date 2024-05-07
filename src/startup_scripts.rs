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
        r#"#!/bin/bash
set -e

# start faucet
nohup solana-faucet --keypair bootstrap-accounts/faucet.json &

# Start the bootstrap validator node
# shellcheck disable=SC1091
source /home/solana/k8s-cluster-scripts/common.sh

no_restart=0

# Define the paths to the validator cli. pre 1.18 is `solana-validator`. post 1.18 is `agave-validator`
agave_validator="/home/solana/.cargo/bin/agave-validator"
solana_validator="/home/solana/.cargo/bin/solana-validator"

# Initialize program variable
program=""

# Check if agave-validator exists and is executable
if [[ -x "$agave_validator" ]]; then
    program="agave-validator"
elif [[ -x "$solana_validator" ]]; then
    program="solana-validator"
else
    echo "Neither agave-validator nor solana-validator could be found or is not executable."
    exit 1
fi

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

    pub fn validator() -> &'static str {
        r#"#!/bin/bash

# Start Validator
# shellcheck disable=SC1091
source /home/solana/k8s-cluster-scripts/common.sh

args=(
  --max-genesis-archive-unpacked-size 1073741824
  --no-poh-speed-test
  --no-os-network-limits-test
)
airdrops_enabled=1
node_sol=
stake_sol=
identity=validator-accounts/identity.json
vote_account=validator-accounts/vote.json
no_restart=0
gossip_entrypoint=$BOOTSTRAP_GOSSIP_ADDRESS
ledger_dir=/home/solana/ledger
faucet_address=$LOAD_BALANCER_FAUCET_ADDRESS

# Define the paths to the validator cli. pre 1.18 is `solana-validator`. post 1.18 is `agave-validator`
agave_validator="/home/solana/.cargo/bin/agave-validator"
solana_validator="/home/solana/.cargo/bin/solana-validator"

# Initialize program variable
program=""

# Check if agave-validator exists and is executable
if [[ -x "$agave_validator" ]]; then
    program="agave-validator"
elif [[ -x "$solana_validator" ]]; then
    program="solana-validator"
else
    echo "Neither agave-validator nor solana-validator could be found or is not executable."
    exit 1
fi

echo "PROGRAM: $program"

usage() {
  if [[ -n $1 ]]; then
    echo "$*"
    echo
  fi
  cat <<EOF
usage: $0 [OPTIONS] [cluster entry point hostname]
Start a validator with no stake
OPTIONS:
  --ledger PATH             - store ledger under this PATH
  --init-complete-file FILE - create this file, if it doesn't already exist, once node initialization is complete
  --node-sol SOL            - Number of SOL this node has been funded from the genesis config (default: $node_sol)
  --no-voting               - start node without vote signer
  --rpc-port port           - custom RPC port for this node
  --no-restart              - do not restart the node if it exits
  --no-airdrop              - The genesis config has an account for the node. Airdrops are not required.
EOF
  exit 1
}

positional_args=()
while [[ -n $1 ]]; do
  if [[ ${1:0:1} = - ]]; then
    if [[ $1 = --no-restart ]]; then
      no_restart=1
      shift
    elif [[ $1 = --no-airdrop ]]; then
      airdrops_enabled=0
      shift
    elif [[ $1 == --internal-node-stake-sol ]]; then
      stake_sol=$2
      shift 2
    elif [[ $1 == --internal-node-sol ]]; then
      node_sol=$2
      shift 2
    # agave-validator options
    elif [[ $1 = --expected-genesis-hash ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --expected-shred-version ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --identity ]]; then
      identity=$2
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --authorized-voter ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --authorized-withdrawer ]]; then
      authorized_withdrawer=$2
      shift 2
    elif [[ $1 = --vote-account ]]; then
      vote_account=$2
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --init-complete-file ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --ledger ]]; then
      ledger_dir=$2
      shift 2
    elif [[ $1 = --entrypoint ]]; then
      gossip_entrypoint=$2
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --no-snapshot-fetch ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --no-voting ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --dev-no-sigverify ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --dev-halt-at-slot ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --rpc-port ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --rpc-faucet-address ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --accounts ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --gossip-port ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --dynamic-port-range ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --snapshot-interval-slots ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --maximum-snapshots-to-retain ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --limit-ledger-size ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --no-rocksdb-compaction ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --enable-rpc-transaction-history ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --enable-cpi-and-log-storage ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --enable-extended-tx-metadata-storage ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --skip-poh-verify ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --tpu-disable-quic ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --tpu-enable-udp ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --rpc-send-batch-ms ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --rpc-send-batch-size ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --log ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --known-validator ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 = --halt-on-known-validators-accounts-hash-mismatch ]]; then
      args+=("$1")
      shift
    elif [[ $1 = --max-genesis-archive-unpacked-size ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --wait-for-supermajority ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --expected-bank-hash ]]; then
      args+=("$1" "$2")
      shift 2
    elif [[ $1 == --accounts-db-skip-shrink ]]; then
      args+=("$1")
      shift
    elif [[ $1 == --require-tower ]]; then
      args+=("$1")
      shift
    elif [[ $1 = -h ]]; then
      usage "$@"
    else
      echo "Unknown argument: $1"
      exit 1
    fi
  else
    positional_args+=("$1")
    shift
  fi
done

echo "post positional args"
if [[ ${#positional_args[@]} -gt 1 ]]; then
  usage "$@"
fi

if [[ -n $REQUIRE_KEYPAIRS ]]; then
  if [[ -z $identity ]]; then
    usage "Error: --identity not specified"
  fi
  if [[ -z $vote_account ]]; then
    usage "Error: --vote-account not specified"
  fi
  if [[ -z $authorized_withdrawer ]]; then
    usage "Error: --authorized_withdrawer not specified"
  fi
fi

if [[ -n $gossip_entrypoint ]]; then
  # Prefer the --entrypoint argument if supplied...
  if [[ ${#positional_args[@]} -gt 0 ]]; then
    usage "$@"
  fi
else
  # ...but also support providing the entrypoint's hostname as the first
  #    positional argument
  entrypoint_hostname=${positional_args[0]}
  if [[ -z $entrypoint_hostname ]]; then
    gossip_entrypoint=127.0.0.1:8001
  else
    gossip_entrypoint="$entrypoint_hostname":8001
  fi
fi

echo "gossip entrypoint: $gossip_entrypoint"
default_arg --entrypoint "$gossip_entrypoint"
if ((airdrops_enabled)); then
  default_arg --rpc-faucet-address "$faucet_address"
  echo "airdrops enabled adding rpc-faucet-address: $faucet_address"
fi

default_arg --identity "$identity"
default_arg --vote-account "$vote_account"
default_arg --ledger "$ledger_dir"
default_arg --log -
default_arg --full-rpc-api
default_arg --no-incremental-snapshots
default_arg --allow-private-addr
default_arg --gossip-port 8001
default_arg --rpc-port 8899
default_arg --enable-rpc-transaction-history

PS4="$(basename "$0"): "
echo "PS4: $PS4"

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

# Maximum number of retries
MAX_RETRIES=30

# Delay between retries (in seconds)
RETRY_DELAY=5

# Load balancer RPC URL
LOAD_BALANCER_RPC_URL="http://$LOAD_BALANCER_RPC_ADDRESS"

# Identity file
IDENTITY_FILE=$identity

vote_account_already_exists=false
stake_account_already_exists=false

# Function to run a Solana command with retries. need reties because sometimes dns resolver fails
# if pod dies and starts up again it may try to create a vote account or something that already exists
run_solana_command() {
    local command="$1"
    local description="$2"

    for ((retry_count = 1; retry_count <= MAX_RETRIES; retry_count++)); do
      echo "Attempt $retry_count for: $description"

      # Capture both stdout and stderr in $output
      output=$($command 2>&1)
      status=$?

      if [ $status -eq 0 ]; then
          echo "Command succeeded: $description"
          return 0
      else
        echo "Command failed for: $description (Exit status $status)"
        echo "$output" # Print the output which includes the error

        # Check for specific error message
        if [[ "$output" == *"Vote account"*"already exists"* ]]; then
            echo "Vote account already exists. Continuing without exiting."
            vote_account_already_exists=true
            return 0
        fi
        if [[ "$output" == *"Stake account"*"already exists"* ]]; then
            echo "Stake account already exists. Continuing without exiting."
            stake_account_already_exists=true
            return 0
        fi

        if [ "$retry_count" -lt $MAX_RETRIES ]; then
          echo "Retrying in $RETRY_DELAY seconds..."
          sleep $RETRY_DELAY
        fi
      fi
    done

    echo "Max retry limit reached. Command still failed for: $description"
    return 1
}

setup_validator() {
  if ! run_solana_command "solana -u $LOAD_BALANCER_RPC_URL airdrop $node_sol $IDENTITY_FILE" "Airdrop"; then
    echo "Aidrop command failed."
    exit 1
  fi

  if ! run_solana_command "solana -u $LOAD_BALANCER_RPC_URL create-vote-account --allow-unsafe-authorized-withdrawer validator-accounts/vote.json $IDENTITY_FILE $IDENTITY_FILE -k $IDENTITY_FILE" "Create Vote Account"; then
    if $vote_account_already_exists; then
      echo "Vote account already exists. Skipping remaining commands."
    else
      echo "Create vote account failed."
      exit 1
    fi
  fi

  echo "created vote account"
}

run_delegate_stake() {
  echo "stake sol for account: $stake_sol"
  if ! run_solana_command "solana -u $LOAD_BALANCER_RPC_URL create-stake-account validator-accounts/stake.json $stake_sol -k $IDENTITY_FILE" "Create Stake Account"; then
    if $stake_account_already_exists; then
      echo "Stake account already exists. Skipping remaining commands."
    else
      echo "Create stake account failed."
      exit 1
    fi
  fi
  echo "created stake account"

  if [ "$stake_account_already_exists" != true ]; then
    echo "stake account does not exist. so lets deligate"
    if ! run_solana_command "solana -u $LOAD_BALANCER_RPC_URL delegate-stake validator-accounts/stake.json validator-accounts/vote.json --force -k $IDENTITY_FILE" "Delegate Stake"; then
      echo "Delegate stake command failed."
      exit 1
    fi
    echo "delegated stake"
  fi

  solana --url $LOAD_BALANCER_RPC_URL --keypair $IDENTITY_FILE stakes validator-accounts/stake.json
}

echo "get airdrop and create vote account"
setup_validator
echo "create stake account and delegate stake"
run_delegate_stake 

echo running validator:

echo "Validator Args"
for arg in "${args[@]}"; do
  echo "$arg"
done

while true; do
  echo "$PS4$program ${args[*]}"

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

    pub fn rpc() -> &'static str {
        r#"#!/bin/bash
set -e

nohup solana-faucet --keypair non-voting-validator-accounts/faucet.json &

# Start Validator
# shellcheck disable=SC1091
source /home/solana/k8s-cluster-scripts/common.sh

args=(
    --max-genesis-archive-unpacked-size 1073741824
    --no-poh-speed-test
    --no-os-network-limits-test
    --no-voting
)
airdrops_enabled=1
node_sol=500 # 500 SOL: number of SOL to airdrop the node for transaction fees and vote account rent exemption (ignored if airdrops_enabled=0)
identity=non-voting-validator-accounts/identity.json
no_restart=0
gossip_entrypoint=$BOOTSTRAP_GOSSIP_ADDRESS
ledger_dir=/home/solana/ledger
# faucet_address=$BOOTSTRAP_FAUCET_ADDRESS
faucet_address=$LOAD_BALANCER_FAUCET_ADDRESS

# Define the paths to the validator cli. pre 1.18 is `solana-validator`. post 1.18 is `agave-validator`
agave_validator="/home/solana/.cargo/bin/agave-validator"
solana_validator="/home/solana/.cargo/bin/solana-validator"

# Initialize program variable
program=""

# Check if agave-validator exists and is executable
if [[ -x "$agave_validator" ]]; then
    program="agave-validator"
elif [[ -x "$solana_validator" ]]; then
    program="solana-validator"
else
    echo "Neither agave-validator nor solana-validator could be found or is not executable."
    exit 1
fi

echo "program: $program"

usage() {
    if [[ -n $1 ]]; then
    echo "$*"
    echo
    fi
    cat <<EOF
usage: $0 [OPTIONS] [cluster entry point hostname]
Start a validator with no stake
OPTIONS:
    --ledger PATH             - store ledger under this PATH
    --init-complete-file FILE - create this file, if it doesn't already exist, once node initialization is complete
    --node-sol SOL            - Number of SOL this node has been funded from the genesis config (default: $node_sol)
    --no-voting               - start node without vote signer
    --rpc-port port           - custom RPC port for this node
    --no-restart              - do not restart the node if it exits
    --no-airdrop              - The genesis config has an account for the node. Airdrops are not required.
EOF
    exit 1
}

positional_args=()
while [[ -n $1 ]]; do
    if [[ ${1:0:1} = - ]]; then
    if [[ $1 = --no-restart ]]; then
        no_restart=1
        shift
    elif [[ $1 = --no-airdrop ]]; then
        airdrops_enabled=0
        shift
    elif [[ $1 == --internal-node-sol ]]; then
        node_sol=$2
        shift 2
    # agave-validator options
    elif [[ $1 = --expected-genesis-hash ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --expected-shred-version ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --identity ]]; then
        identity=$2
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --authorized-withdrawer ]]; then
        authorized_withdrawer=$2
        shift 2
    elif [[ $1 = --init-complete-file ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --ledger ]]; then
        ledger_dir=$2
        shift 2
    elif [[ $1 = --entrypoint ]]; then
        gossip_entrypoint=$2
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --no-snapshot-fetch ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --no-voting ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --dev-no-sigverify ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --dev-halt-at-slot ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --rpc-port ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --rpc-faucet-address ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --accounts ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --gossip-port ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --dynamic-port-range ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --snapshot-interval-slots ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --maximum-snapshots-to-retain ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --limit-ledger-size ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --no-rocksdb-compaction ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --enable-rpc-transaction-history ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --enable-cpi-and-log-storage ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --enable-extended-tx-metadata-storage ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --skip-poh-verify ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --tpu-disable-quic ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --tpu-enable-udp ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --rpc-send-batch-ms ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --rpc-send-batch-size ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --log ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --known-validator ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 = --halt-on-known-validators-accounts-hash-mismatch ]]; then
        args+=("$1")
        shift
    elif [[ $1 = --max-genesis-archive-unpacked-size ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 == --wait-for-supermajority ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 == --expected-bank-hash ]]; then
        args+=("$1" "$2")
        shift 2
    elif [[ $1 == --accounts-db-skip-shrink ]]; then
        args+=("$1")
        shift
    elif [[ $1 == --require-tower ]]; then
        args+=("$1")
        shift
    elif [[ $1 = -h ]]; then
        usage "$@"
    else
        echo "Unknown argument: $1"
        exit 1
    fi
    else
    positional_args+=("$1")
    shift
    fi
done

echo "post positional args"
if [[ "$SOLANA_GPU_MISSING" -eq 1 ]]; then
    echo "Testnet requires GPUs, but none were found!  Aborting..."
    exit 1
fi

if [[ ${#positional_args[@]} -gt 1 ]]; then
    usage "$@"
fi

if [[ -n $REQUIRE_KEYPAIRS ]]; then
    if [[ -z $identity ]]; then
    usage "Error: --identity not specified"
    fi
    if [[ -z $authorized_withdrawer ]]; then
    usage "Error: --authorized_withdrawer not specified"
    fi
fi

if [[ -n $gossip_entrypoint ]]; then
    # Prefer the --entrypoint argument if supplied...
    if [[ ${#positional_args[@]} -gt 0 ]]; then
    usage "$@"
    fi
else
    # ...but also support providing the entrypoint's hostname as the first
    #    positional argument
    entrypoint_hostname=${positional_args[0]}
    if [[ -z $entrypoint_hostname ]]; then
    gossip_entrypoint=127.0.0.1:8001
    else
    gossip_entrypoint="$entrypoint_hostname":8001
    fi
fi

default_arg --entrypoint "$gossip_entrypoint"
if ((airdrops_enabled)); then
    default_arg --rpc-faucet-address "$faucet_address"
fi

default_arg --identity "$identity"
default_arg --ledger "$ledger_dir"
default_arg --log -
default_arg --full-rpc-api
default_arg --no-incremental-snapshots
default_arg --allow-private-addr
default_arg --gossip-port 8001
default_arg --rpc-port 8899

# set -e
PS4="$(basename "$0"): "
echo "PS4: $PS4"

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

echo "All commands succeeded. Running agave-validator next..."

echo "Validator Args"
for arg in "${args[@]}"; do
    echo "$arg"
done

while true; do
    echo "$PS4$program ${args[*]}"

    $program "${args[@]}" &
    pid=$!
    echo "pid: $pid"

    if ((no_restart)); then
    wait "$pid"
    exit $?
    fi

    while true; do
    if [[ -z $pid ]] || ! kill -0 "$pid"; then
        echo "\############## non voting validator exited, restarting ##############"
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
