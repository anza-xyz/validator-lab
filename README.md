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
    --deploy-method local
    --local-path <path-to-local-agave-monorepo>
```

#### Build specific Agave release
```
cargo run --bin cluster --
    -n <namespace>
    --deploy-method tar
    --release-channel <agave-version: e.g. v1.17.28> # note: MUST include the "v" 
```
