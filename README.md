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

#### Note on `--cluster-data-path`:
`--cluster-data-path` can just be an empty directory. It will be used to store:
1) Validator, client, rpc, and faucet account(s)
2) Genesis
3) Validator, client, and rpc Dockerfiles

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
```

#### Client bench-tps-args
Client accounts are funded on deployment of the client.

Command Examples:
For client version < 2.0.0 && client version > 1.17.0
```
--bench-tps-args 'tx-count=5000 keypair-multiplier=4 threads=16 num-lamports-per-account=200000000 sustained tpu-connection-pool-size=8 thread-batch-sleep-ms=0'
```

For client Version >= 2.0.0
```
--bench-tps-args 'tx-count=5000 keypair-multiplier=4 threads=16 num-lamports-per-account=200000000 sustained tpu-connection-pool-size=8 thread-batch-sleep-ms=0 commitment-config=processed'
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
For example, say you want to deploy a cluster with the following nodes:
* 1 bootstrap, 3 validators, 1 rpc-node, and 1 client running some agave-repo local commit
* 5 validators and 4 rpc nodes running v1.18.15
* 20 clients running v1.18.14

Each set of validators and clients get deployed individually by version. But they will all run in the same cluster

1) Deploy a local cluster as normal:
   * Specify how many validators, rpc nodes, and clients you want running v1.18.14
```
cargo run --bin cluster -- -n <namespace> --registry <registry> --local-path /home/sol/solana --num-validators 3 --num-rpc-nodes 1 --cluster-data-path /home/sol/validator-lab-build/ --num-clients 1 --client-type tpu-client --client-to-run bench-tps --bench-tps-args 'tx-count=5000 threads=4 thread-batch-sleep-ms=0'
```
2) Deploy a set of 5 validators running a different validator version (e.g. v1.18.15)
    * Must pass in `--no-bootstrap` so we don't recreate the genesis and deploy another bootstrap
```
cargo run --bin cluster -- -n <namespace> --registry <registry> --release-channel v1.18.15 --num-validators 5 --num-rpc-nodes 4 --cluster-data-path /home/sol/validator-lab-build/ --no-bootstrap
```
3) Deploy the final set of clients running v1.18.14 these 20 clients will load the cluster you deployed in (1) and (2)
    * Must pass in `--no-bootstrap` so we don't recreate the genesis and deploy another bootstrap
```
cargo run --bin cluster -- -n <namespace> --registry <registry> --release-channel v1.18.14 --cluster-data-path /home/sol/validator-lab-build/ --num-clients 20 --client-type tpu-client --client-to-run bench-tps --bench-tps-args 'tx-count=10000 threads=16 thread-batch-sleep-ms=0' --no-bootstrap
```

For steps (2) and (3), when using `--no-bootstrap`, we assume that the directory at `--cluster-data-path <directory>` has the correct genesis, bootstrap identity, and faucet account stored. These are all created in step (1).

Note: We can't deploy heterogeneous clusters across v1.17 and v1.18 due to feature differences. Hope to fix this in the future. Have something where we can specifically define which features to enable.

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