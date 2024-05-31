# Validator Lab
### Deploy Validator Clusters for Testing

#### About
In Validator Lab we can deploy and test new validator features quickly and easily. Validator Lab will take your modified validator code and deploy a cluster of validators running in Kubernetes pods on nodes all around the world. This allows us to spin up and tear down over a thousand validators with ease and with little user intervention.

### Disclaimer:
- This library is a work in progress. It will be built over a series of PRs. See [PROGRESS.md](PROGRESS.md) for roadmap and progress

## How to run

### Setup
Ensure you have the proper permissions to conenct to the Monogon Kubernetes endpoint. Reach out to Leo on slack if you need the key (you do if you haven't asked him in the past).

From your local build host, login to Docker for pushing/pulling repos. Currently we just use the users own Docker login. This will likely change in the future.
```
docker login
```

```
kubectl create ns <namespace>
```

### Run
#### Build Agave from local agave repo
```
cargo run --bin cluster --
    -n <namespace>
    --local-path <path-to-local-agave-monorepo>
    --cluster-data-path <path-to-directory-to-store-cluster-accounts-genesis-etc>
```

#### Build specific Agave release
```
cargo run --bin cluster --
    -n <namespace>
    --release-channel <agave-version: e.g. v1.17.28> # note: MUST include the "v"
    --cluster-data-path <path-to-directory-to-store-cluster-accounts-genesis-etc>
```

#### Build from Local Repo and Configure Genesis and Bootstrap and Validator Image
Example:
```
cargo run --bin cluster -- 
    -n <namespace> 
    --local-path /home/sol/solana
    --cluster-data-path /home/sol/validator-lab-build
    --num_validators <number-of-non-bootstrap-voting-validators>
    # genesis config. Optional: Many of these have defaults
    --hashes-per-tick <hashes-per-tick>
    --faucet-lamports <faucet-lamports>
    --bootstrap-validator-sol <validator-sol>
    --bootstrap-validator-stake-sol <validator-stake>
    --max-genesis-archive-unpacked-size <size in bytes>
    --target-lamports-per-signature <lamports-per-signature>
    --slots-per-epoch <slots-per-epoch>
    # docker config
    --registry <docker-registry>        # e.g. gregcusack 
    --base-image <base-image>           # e.g. ubuntu:20.04
    --image-name <docker-image-name>    # e.g. cluster-image
    # validator config
    --full-rpc
    --internal-node-sol <Sol>
    --internal-node-stake-sol <Sol>
    # kubernetes config
    --cpu-requests <cores>
    --memory-requests <memory>
    # deploy with clients
    -c <num-clients>
    --client-type <client-type e.g. tpu-client>
    --client-to-run <type-of-client e.g. bench-tps>
    --client-wait-for-n-nodes <wait-for-N-nodes-to-converge-before-starting-client>
    --bench-tps-args <bench-tps-args e.g. tx-count=25000>
    --run-client
```

## Metrics
1) Setup metrics database:
```
./init-metrics -c <database-name> -u <metrics-username>
# enter password when prompted
```
2) add the following to your `cluster` command from above
```
--metrics-host https://internal-metrics.solana.com # need the `https://` here
--metrics-port 8086
--metrics-db <database-name>            # from (1)
--metrics-username <metrics-username>   # from (1)
--metrics-password <metrics-password>   # from (1)
```

#### RPC Nodes
You can add in RPC nodes. These sit behind a load balancer. Load balancer distributed loads across all RPC nodes and the bootstrap. Set the number of RPC nodes with:
```
--num-rpc-nodes <num-nodes>
```

## Heterogeneous Clusters
You can deploy a cluster with heterogeneous validator versions
Additional flags required for a heterogeneous cluster:
    - `--no-bootstrap`: do not deploy a bootstrap validator
    - `--run-client`: deploy and run a client

For example, say you want to deploy a cluster running some agave-repo local commit and v1.18.14
1) Deploy a local cluster as normal:
   * Specify how many validators, rpc nodes, and clients you want. 
   * We will be creating client accounts but not deploying the clients yet. As a result, you need to specify `tx-count` in the client configuration to generate `tx-count * 8` client accounts.
```
cargo run --bin cluster -- -n <namespace> --registry <registry> --local-path /home/sol/solana --num-validators 3 --num-rpc-nodes 1 --cluster-data-path /home/sol/validator-lab-build/ --num-clients 1 --client-type tpu-client --client-to-run bench-tps --bench-tps-args 'tx-count=10000'
```
1) Deploy a second set of validators running a different validator version
    * Must pass in `--no-bootstrap` so we don't recreate the genesis and deploy another bootstrap
    * If this is the last set of validators you want to deploy, pass in `--run-client` along with your client configurations to deploy and execute the client.
    * If it is not the last set of validators you want to deploy in this cluster, just leave out all client configurations. 
```
cargo run --bin cluster -- -n <namespace> --registry <registry> --release-channel v1.18.14 --num-validators 3 --cluster-data-path /home/sol/validator-lab-build/ --no-bootstrap --run-client --num-clients 1 --client-type tpu-client --client-to-run bench-tps --bench-tps-args 'tx-count=10000 threads=8 thread-batch-sleep-ms=250'
```

## Kubernetes Cheatsheet
Create namespace:
```
kubectl create ns <namespace>
```

Delete namespace:
```
kubectl delete ns <namespace>
```

Get running pods:
```
kubectl get pods -n <namespace>
```

Get pod logs:
```
kubectl logs -n <namespace> <pod-name>
```

Exec into pod:
```
kubectl exec -it -n <namespace> <pod-name> -- /bin/bash
```

Get information about pod:
```
kubectl describe pod -n <namespace> <pod-name>
```