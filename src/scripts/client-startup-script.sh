#!/bin/bash

clientToRun="$1"
benchTpsExtraArgs="$2"
clientType=

# check if benchTpsExtraArgs is set. if not then it will get set to client-type. Which then needs to get handled appropriately
if [[ "$benchTpsExtraArgs" == "tpu-client" || "$benchTpsExtraArgs" == "rpc-client" ]]; then
    clientType=$benchTpsExtraArgs
    benchTpsExtraArgs=
    shift 2
else
    clientType="${3:-tpu-client}"
    shift 3
    # Convert string to array
    IFS=' ' read -r -a argsArray <<< "$benchTpsExtraArgs"

    # Loop through the array and check for the specific flag
    for arg in "${argsArray[@]}"; do
        if [ "$arg" == "--use-rpc-client" ]; then
            clientType="rpc-client"
            break
        elif [ "$arg" == "--use-tpu-client" ]; then
            clientType="tpu-client"
            break
        fi
    done
fi

runtime_args=()
while [[ -n $1 ]]; do
  if [[ ${1:0:1} = - ]]; then
    if [[ $1 = --target-node ]]; then
      echo "WARNING: --target-node not supported yet...not included"
      shift 2
    elif [[ $1 = --duration ]]; then
      runtime_args+=("$1" "$2")
      shift 2
    elif [[ $1 = --num-nodes ]]; then
      runtime_args+=("$1" "$2")
      shift 2
    else
      echo "Unknown argument: $1"
      solana-bench-tps --help
      exit 1
    fi
  else
    echo "Unknown argument: $1"
    solana-bench-tps --help
    exit 1
  fi
done

echo "get airdrop for client"
solana airdrop 5000000 -k ./client-accounts/identity.json  -u "http://$LOAD_BALANCER_RPC_ADDRESS"

missing() {
  echo "Error: $1 not specified"
  exit 1
}

threadCount=$(nproc)
if [[ $threadCount -gt 4 ]]; then
  threadCount=4
fi

echo "threadCount: $threadCount"

RPC_CLIENT=false
case "$clientType" in
  tpu-client)
    RPC_CLIENT=false
    ;;
  rpc-client)
    RPC_CLIENT=true
    ;;
  *)
    echo "Unexpected clientType: \"$clientType\""
    exit 1
    ;;
esac
case $clientToRun in
bench-tps)
  args=()

  if ${RPC_CLIENT}; then
    args+=(--use-rpc-client)
  fi

  entrypointIp="${BOOTSTRAP_GOSSIP_ADDRESS:0:-5}"
  url="$entrypointIp:8899"

  args+=(--bind-address "$entrypointIp")
  args+=(--client-node-id ./client-accounts/identity.json)

  clientCommand="\
    solana-bench-tps \
      --sustained \
      --threads $threadCount \
      $benchTpsExtraArgs \
      --read-client-keys ./client-accounts.yml \
      --url "http://$url"
      ${args[*]} \
      ${runtime_args[*]} \
  "
  ;;
idle)
  # In net/remote/remote-client.sh, we add faucet keypair here
  # but in this case we already do that in the docker container
  # by default
  while true; do sleep 3600; done
  ;;
*)
  echo "Unknown client name: $clientToRun"
  exit 1
esac

echo "client command to run: $clientCommand"
$clientCommand
