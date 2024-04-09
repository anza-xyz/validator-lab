use {
    clap::{command, value_t_or_exit, Arg, ArgGroup},
    log::*,
    solana_ledger::blockstore_cleanup_service::{
        DEFAULT_MAX_LEDGER_SHREDS, DEFAULT_MIN_MAX_LEDGER_SHREDS,
    },
    solana_sdk::{pubkey::Pubkey, signature::keypair::read_keypair_file, signer::Signer},
    std::{fs, thread, time::Duration},
    strum::VariantNames,
    validator_lab::{
        client_config::{parse_and_format_bench_tps_args, ClientConfig},
        docker::{DockerConfig, DockerImage},
        genesis::{
            Genesis, GenesisFlags, DEFAULT_CLIENT_LAMPORTS_PER_SIGNATURE,
            DEFAULT_INTERNAL_NODE_SOL, DEFAULT_INTERNAL_NODE_STAKE_SOL,
        },
        kubernetes::{Kubernetes, PodRequests},
        ledger_helper::LedgerHelper,
        release::{BuildConfig, BuildType, DeployMethod},
        validator::{LabelType, Validator},
        validator_config::ValidatorConfig,
        Metrics, SolanaRoot, ValidatorType,
    },
};

fn parse_matches() -> clap::ArgMatches {
    command!()
        .arg(
            Arg::new("cluster_namespace")
                .long("namespace")
                .short('n')
                .takes_value(true)
                .default_value("default")
                .help("namespace to deploy test cluster"),
        )
        .arg(
            Arg::with_name("number_of_validators")
                .long("num-validators")
                .takes_value(true)
                .default_value("1")
                .help("Number of validator replicas to deploy")
                .validator(|s| match s.parse::<i32>() {
                    Ok(n) if n > 0 => Ok(()),
                    _ => Err(String::from("number_of_validators should be >= 0")),
                }),
        )
        .arg(
            Arg::new("local_path")
                .long("local-path")
                .takes_value(true)
                .conflicts_with("release_channel")
                .help("Build validator from local Agave repo. Specify path here."),
        )
        .arg(
            Arg::new("build_type")
                .long("build-type")
                .takes_value(true)
                .possible_values(BuildType::VARIANTS)
                .default_value(BuildType::Release.into())
                .help("Specifies the build type: skip, debug, or release."),
        )
        .arg(
            Arg::with_name("release_channel")
                .long("release-channel")
                .takes_value(true)
                .conflicts_with("local_path")
                .help("Pulls specific release version. e.g. v1.17.2"),
        )
        .group(
            ArgGroup::new("required_group")
                .args(&["local_path", "release_channel"])
                .required(true),
        )
        // Genesis Config
        .arg(
            Arg::with_name("hashes_per_tick")
                .long("hashes-per-tick")
                .takes_value(true)
                .default_value("auto")
                .help("NUM_HASHES|sleep|auto - Override the default --hashes-per-tick for the cluster"),
        )
        .arg(
            Arg::with_name("slots_per_epoch")
                .long("slots-per-epoch")
                .takes_value(true)
                .help("override the number of slots in an epoch"),
        )
        .arg(
            Arg::with_name("target_lamports_per_signature")
                .long("target-lamports-per-signature")
                .takes_value(true)
                .help("Genesis config. target lamports per signature"),
        )
        .arg(
            Arg::with_name("faucet_lamports")
                .long("faucet-lamports")
                .takes_value(true)
                .help("Override the default 500000000000000000 lamports minted in genesis"),
        )
        .arg(
            Arg::with_name("enable_warmup_epochs")
                .long("enable-warmup-epochs")
                .takes_value(true)
                .possible_values(&["true", "false"])
                .default_value("true")
                .help("Genesis config. enable warmup epoch. defaults to true"),
        )
        .arg(
            Arg::with_name("max_genesis_archive_unpacked_size")
                .long("max-genesis-archive-unpacked-size")
                .takes_value(true)
                .help("Genesis config. max_genesis_archive_unpacked_size"),
        )
        .arg(
            Arg::with_name("cluster_type")
                .long("cluster-type")
                .possible_values(&["development", "devnet", "testnet", "mainnet-beta"])
                .takes_value(true)
                .default_value("development")
                .help(
                    "Selects the features that will be enabled for the cluster"
                ),
        )
        .arg(
            Arg::with_name("bootstrap_validator_sol")
                .long("bootstrap-validator-sol")
                .takes_value(true)
                .help("Genesis config. bootstrap validator sol"),
        )
        .arg(
            Arg::with_name("bootstrap_validator_stake_sol")
                .long("bootstrap-validator-stake-sol")
                .takes_value(true)
                .help("Genesis config. bootstrap validator stake sol"),
        )
        //Docker config
        .arg(
            Arg::with_name("docker_build")
                .long("docker-build")
                .requires("registry_name")
                .help("Build Docker images. Build new docker images"),
        )
        .arg(
            Arg::with_name("registry_name")
                .long("registry")
                .takes_value(true)
                .required(true)
                .help("Registry to push docker image to"),
        )
        .arg(
            Arg::with_name("image_name")
                .long("image-name")
                .takes_value(true)
                .default_value("k8s-cluster-image")
                .help("Docker image name. Will be prepended with validator_type (bootstrap or validator)"),
        )
        .arg(
            Arg::with_name("base_image")
                .long("base-image")
                .takes_value(true)
                .default_value("ubuntu:20.04")
                .help("Docker base image"),
        )
        .arg(
            Arg::with_name("image_tag")
                .long("tag")
                .takes_value(true)
                .default_value("latest")
                .help("Docker image tag."),
        )
        // Bootstrap/Validator Config
        .arg(
            Arg::with_name("tpu_enable_udp")
                .long("tpu-enable-udp")
                .help("Validator config. Enable UDP for tpu transactions."),
        )
        .arg(
            Arg::with_name("tpu_disable_quic")
                .long("tpu-disable-quic")
                .help("Validator config. Disable quic for tpu packet forwarding"),
        )
        .arg(
            Arg::with_name("limit_ledger_size")
                .long("limit-ledger-size")
                .takes_value(true)
                .help("Validator Config. The `--limit-ledger-size` parameter allows you to specify how many ledger
                shreds your node retains on disk. If you do not
                include this parameter, the validator will keep the entire ledger until it runs
                out of disk space. The default value attempts to keep the ledger disk usage
                under 500GB. More or less disk usage may be requested by adding an argument to
                `--limit-ledger-size` if desired. Check `agave-validator --help` for the
                default limit value used by `--limit-ledger-size`. More information about
                selecting a custom limit value is at : https://github.com/solana-labs/solana/blob/583cec922b6107e0f85c7e14cb5e642bc7dfb340/core/src/ledger_cleanup_service.rs#L15-L26"),
        )
        .arg(
            Arg::with_name("skip_poh_verify")
                .long("skip-poh-verify")
                .help("Validator config. If set, validators will skip verifying
                the ledger they already have saved to disk at
                boot (results in a much faster boot)"),
        )
        .arg(
            Arg::with_name("no_snapshot_fetch")
                .long("no-snapshot-fetch")
                .help("Validator config. If set, disables booting validators from a snapshot"),
        )
        .arg(
            Arg::with_name("require_tower")
                .long("require-tower")
                .help("Validator config. Refuse to start if saved tower state is not found.
                Off by default since validator won't restart if the pod restarts"),
        )
        .arg(
            Arg::with_name("enable_full_rpc")
                .long("full-rpc")
                .help("Validator config. Support full RPC services on all nodes"),
        )
        .arg(
            Arg::with_name("internal_node_sol")
                .long("internal-node-sol")
                .takes_value(true)
                .help("Amount to fund internal nodes in genesis config."),
        )
        .arg(
            Arg::with_name("internal_node_stake_sol")
                .long("internal-node-stake-sol")
                .takes_value(true)
                .help("Amount to stake internal nodes (Sol)."),
        )
        //RPC config
        .arg(
            Arg::with_name("number_of_rpc_nodes")
                .long("num-rpc-nodes")
                .takes_value(true)
                .default_value("0")
                .help("Number of rpc nodes")
                .validator(|s| match s.parse::<i32>() {
                    Ok(n) if n >= 0 => Ok(()),
                    _ => Err(String::from("number_of_rpc_nodes should be >= 0")),
                }),
        )
        // Client Config
        .arg(
            Arg::with_name("number_of_clients")
                .long("num-clients")
                .short('c')
                .takes_value(true)
                .default_value("0")
                .help("Number of clients ")
                .validator(|s| match s.parse::<i32>() {
                    Ok(n) if n >= 0 => Ok(()),
                    _ => Err(String::from("number_of_clients should be >= 0")),
                }),
        )
        .arg(
            Arg::with_name("client_delay_start")
                .long("client-delay-start")
                .takes_value(true)
                .default_value("0")
                .help("Number of seconds to wait after validators have finished starting before starting client programs
                (default: 0")
                .validator(|s| match s.parse::<i32>() {
                    Ok(n) if n >= 0 => Ok(()),
                    _ => Err(String::from("client_delay_start should be greater than 0")),
                }),
        )
        .arg(
            Arg::with_name("client_type")
                .long("client-type")
                .takes_value(true)
                .default_value("tpu-client")
                .possible_values(&["tpu-client", "rpc-client"])
                .help("Client Config. options: tpu-client, rpc-client."),
        )
        .arg(
            Arg::with_name("client_to_run")
                .long("client-to-run")
                .takes_value(true)
                .default_value("bench-tps")
                .possible_values(&["bench-tps", "idle"])
                .help("Client Config. options: idle, bench-tps. default: [bench-tps]"),
        )
        .arg(
            Arg::with_name("bench_tps_args")
                .long("bench-tps-args")
                .value_name("KEY VALUE")
                .takes_value(true)
                .multiple(true)
                .number_of_values(1)
                .help("Client Config.
                User can optionally provide extraArgs that are transparently
                supplied to the client program as command line parameters.
                For example,
                    --bench-tps-args 'tx-count=5000 thread-batch-sleep-ms=250'
                This will start bench-tps clients, and supply '--tx-count 5000 --thread-batch-sleep-ms 250'
                to the bench-tps client."),
        )
        .arg(
            Arg::with_name("target_node")
                .long("target-node")
                .takes_value(true)
                .help("Client Config. Optional: Specify an exact node to send transactions to. use: --target-node <Pubkey>.
                Not supported yet. TODO..."),
        )
        .arg(
            Arg::with_name("duration")
                .long("duration")
                .takes_value(true)
                .default_value("7500")
                .help("Client Config. Seconds to run benchmark, then exit; default is forever use: --duration <SECS>"),
        )
        .arg(
            Arg::with_name("num_nodes")
                .long("num-nodes")
                .short('N')
                .takes_value(true)
                .help("Client Config. Optional: Wait for NUM nodes to converge: --num-nodes <NUM> "),
        )
        // kubernetes config
        .arg(
            Arg::with_name("cpu_requests")
                .long("cpu-requests")
                .takes_value(true)
                .default_value("20") // 20 cores
                .help("Kubernetes pod config. Specify minimum CPUs required for deploying validator.
                    can use millicore notation as well. e.g. 500m (500 millicores) == 0.5 and is equivalent to half a core.
                    [default: 20]"),
        )
        .arg(
            Arg::with_name("memory_requests")
                .long("memory-requests")
                .takes_value(true)
                .default_value("70Gi") // 70 Gigabytes
                .help("Kubernetes pod config. Specify minimum memory required for deploying validator.
                    Can specify unit here (B, Ki, Mi, Gi, Ti) for bytes, kilobytes, etc (2^N notation)
                    e.g. 1Gi == 1024Mi == 1024Ki == 1,047,576B. [default: 70Gi]"),
        )
        //Metrics Config
        .arg(
            Arg::with_name("metrics_host")
                .long("metrics-host")
                .takes_value(true)
                .requires_all(&["metrics_port", "metrics_db", "metrics_username", "metrics_password"])
                .help("Metrics Config. Optional: specify metrics host. e.g. https://internal-metrics.solana.com"),
        )
        .arg(
            Arg::with_name("metrics_port")
                .long("metrics-port")
                .takes_value(true)
                .help("Metrics Config. Optional: specify metrics port. e.g. 8086"),
        )
        .arg(
            Arg::with_name("metrics_db")
                .long("metrics-db")
                .takes_value(true)
                .help("Metrics Config. Optional: specify metrics database. e.g. k8s-cluster-<your name>"),
        )
        .arg(
            Arg::with_name("metrics_username")
                .long("metrics-username")
                .takes_value(true)
                .help("Metrics Config. Optional: specify metrics username"),
        )
        .arg(
            Arg::with_name("metrics_password")
                .long("metrics-password")
                .takes_value(true)
                .help("Metrics Config. Optional: Specify metrics password"),
        )
        .get_matches()
}

#[derive(Clone, Debug)]
pub struct EnvironmentConfig<'a> {
    pub namespace: &'a str,
}

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();
    let matches = parse_matches();
    let environment_config = EnvironmentConfig {
        namespace: matches.value_of("cluster_namespace").unwrap_or_default(),
    };

    let num_validators = value_t_or_exit!(matches, "number_of_validators", usize);
    let num_rpc_nodes = value_t_or_exit!(matches, "number_of_rpc_nodes", usize);

    let client_config = ClientConfig {
        num_clients: value_t_or_exit!(matches, "number_of_clients", usize),
        client_delay_start: matches
            .value_of("client_delay_start")
            .unwrap()
            .parse()
            .expect("Failed to parse client_delay_start into u64"),
        client_type: matches
            .value_of("client_type")
            .unwrap_or_default()
            .to_string(),
        client_to_run: matches
            .value_of("client_to_run")
            .unwrap_or_default()
            .to_string(),
        bench_tps_args: parse_and_format_bench_tps_args(matches.value_of("bench_tps_args")),
        target_node: match matches.value_of("target_node") {
            Some(s) => match s.parse::<Pubkey>() {
                Ok(pubkey) => Some(pubkey),
                Err(e) => return error!("failed to parse pubkey in target_node: {}", e),
            },
            None => None,
        },
        duration: value_t_or_exit!(matches, "duration", u64),
        num_nodes: matches
            .value_of("num_nodes")
            .map(|value_str| value_str.parse().expect("Invalid value for num_nodes")),
    };

    let deploy_method = if let Some(local_path) = matches.value_of("local_path") {
        DeployMethod::Local(local_path.to_owned())
    } else if let Some(release_channel) = matches.value_of("release_channel") {
        DeployMethod::ReleaseChannel(release_channel.to_owned())
    } else {
        unreachable!("One of --local-path or --release-channel must be provided.");
    };

    let solana_root = match &deploy_method {
        DeployMethod::Local(path) => SolanaRoot::new_from_path(path.into()),
        DeployMethod::ReleaseChannel(_) => SolanaRoot::default(),
    };

    let build_type: BuildType = matches
        .value_of_t("build_type")
        .unwrap_or_else(|e| e.exit());

    if let Ok(metadata) = fs::metadata(solana_root.get_root_path()) {
        if !metadata.is_dir() {
            return error!(
                "Build path is not a directory: {:?}",
                solana_root.get_root_path()
            );
        }
    } else {
        return error!(
            "Build directory not found: {:?}",
            solana_root.get_root_path()
        );
    }

    let build_config = BuildConfig::new(
        deploy_method.clone(),
        build_type,
        &solana_root.get_root_path(),
        matches.is_present("docker_build"),
    )
    .unwrap_or_else(|err| {
        panic!("Error creating BuildConfig: {}", err);
    });

    let genesis_flags = GenesisFlags {
        hashes_per_tick: matches
            .value_of("hashes_per_tick")
            .unwrap_or_default()
            .to_string(),
        slots_per_epoch: matches.value_of("slots_per_epoch").map(|value_str| {
            value_str
                .parse()
                .expect("Invalid value for slots_per_epoch")
        }),
        target_lamports_per_signature: matches.value_of("target_lamports_per_signature").map(
            |value_str| {
                value_str
                    .parse()
                    .expect("Invalid value for target_lamports_per_signature")
            },
        ),
        faucet_lamports: matches.value_of("faucet_lamports").map(|value_str| {
            value_str
                .parse()
                .expect("Invalid value for faucet_lamports")
        }),
        enable_warmup_epochs: matches.value_of("enable_warmup_epochs").unwrap() == "true",
        max_genesis_archive_unpacked_size: matches
            .value_of("max_genesis_archive_unpacked_size")
            .map(|value_str| {
                value_str
                    .parse()
                    .expect("Invalid value for max_genesis_archive_unpacked_size")
            }),
        cluster_type: matches
            .value_of("cluster_type")
            .unwrap_or_default()
            .to_string(),
        bootstrap_validator_sol: matches
            .value_of("bootstrap_validator_sol")
            .map(|value_str| {
                value_str
                    .parse()
                    .expect("Invalid value for bootstrap_validator_sol")
            }),
        bootstrap_validator_stake_sol: matches.value_of("bootstrap_validator_stake_sol").map(
            |value_str| {
                value_str
                    .parse()
                    .expect("Invalid value for bootstrap_validator_stake_sol")
            },
        ),
    };

    let mut validator_config = ValidatorConfig {
        tpu_enable_udp: matches.is_present("tpu_enable_udp"),
        tpu_disable_quic: matches.is_present("tpu_disable_quic"),
        internal_node_sol: matches
            .value_of("internal_node_sol")
            .unwrap_or(DEFAULT_INTERNAL_NODE_SOL.to_string().as_str())
            .parse::<f64>()
            .expect("Invalid value for internal_node_stake_sol") as f64,
        internal_node_stake_sol: matches
            .value_of("internal_node_stake_sol")
            .unwrap_or(DEFAULT_INTERNAL_NODE_STAKE_SOL.to_string().as_str())
            .parse::<f64>()
            .expect("Invalid value for internal_node_stake_sol")
            as f64,
        shred_version: None, // set after genesis created
        max_ledger_size: if matches.is_present("limit_ledger_size") {
            let limit_ledger_size = match matches.value_of("limit_ledger_size") {
                Some(_) => value_t_or_exit!(matches, "limit_ledger_size", u64),
                None => DEFAULT_MAX_LEDGER_SHREDS,
            };
            if limit_ledger_size < DEFAULT_MIN_MAX_LEDGER_SHREDS {
                error!(
                    "The provided --limit-ledger-size value was too small, the minimum value is {DEFAULT_MIN_MAX_LEDGER_SHREDS}"
                );
                return;
            }
            Some(limit_ledger_size)
        } else {
            None
        },
        skip_poh_verify: matches.is_present("skip_poh_verify"),
        no_snapshot_fetch: matches.is_present("no_snapshot_fetch"),
        require_tower: matches.is_present("require_tower"),
        enable_full_rpc: matches.is_present("enable_full_rpc"),
        known_validators: None,
    };

    let pod_requests = PodRequests::new(
        matches.value_of("cpu_requests").unwrap().to_string(),
        matches.value_of("memory_requests").unwrap().to_string(),
    );

    let metrics = matches.value_of("metrics_host").map(|host| {
        Metrics::new(
            host.to_string(),
            matches.value_of("metrics_port").unwrap().to_string(),
            matches.value_of("metrics_db").unwrap().to_string(),
            matches.value_of("metrics_username").unwrap().to_string(),
            matches.value_of("metrics_password").unwrap().to_string(),
        )
    });

    let mut kub_controller = Kubernetes::new(
        environment_config.namespace,
        &mut validator_config,
        pod_requests,
        metrics,
        client_config.clone(),
    )
    .await;

    match kub_controller.namespace_exists().await {
        Ok(true) => (),
        Ok(false) => {
            error!(
                "Namespace: '{}' doesn't exist. Exiting...",
                environment_config.namespace
            );
            return;
        }
        Err(err) => {
            error!("Error: {err}");
            return;
        }
    }

    match build_config.prepare().await {
        Ok(_) => info!("Validator setup prepared successfully"),
        Err(err) => {
            error!("Error: {err}");
            return;
        }
    }

    let config_directory = solana_root.get_root_path().join("config-k8s");
    let mut genesis = Genesis::new(config_directory.clone(), genesis_flags);

    match genesis.generate_faucet() {
        Ok(_) => (),
        Err(err) => {
            error!("generate faucet error! {err}");
            return;
        }
    }

    match genesis.generate_accounts(ValidatorType::Bootstrap, 1) {
        Ok(_) => (),
        Err(err) => {
            error!("generate accounts error! {err}");
            return;
        }
    }

    // only create client accounts once
    if client_config.num_clients > 0 {
        match genesis.create_client_accounts(
            client_config.num_clients,
            DEFAULT_CLIENT_LAMPORTS_PER_SIGNATURE,
            &config_directory,
            &deploy_method,
            &solana_root.get_root_path(),
        ) {
            Ok(_) => info!("Client accounts created successfully"),
            Err(err) => {
                error!("generate client accounts error! {err}");
                return;
            }
        }
    }

    // creates genesis and writes to binary file
    match genesis.generate(solana_root.get_root_path(), build_config.build_path()) {
        Ok(_) => (),
        Err(err) => {
            error!("generate genesis error! {err}");
            return;
        }
    }

    match genesis.generate_accounts(ValidatorType::Standard, num_validators) {
        Ok(_) => (),
        Err(err) => {
            error!("generate accounts error! {err}");
            return;
        }
    }

    match genesis.generate_accounts(ValidatorType::RPC, num_rpc_nodes) {
        Ok(_) => (),
        Err(err) => {
            error!("generate rpc accounts error! {err}");
            return;
        }
    }

    let ledger_dir = config_directory.join("bootstrap-validator");
    match LedgerHelper::get_shred_version(&ledger_dir) {
        Ok(shred_version) => kub_controller.set_shred_version(shred_version),
        Err(err) => {
            error!("{err}");
            return;
        }
    }

    //unwraps are safe here. since their requirement is enforced by argmatches
    let docker = DockerConfig::new(
        matches
            .value_of("base_image")
            .unwrap_or_default()
            .to_string(),
        deploy_method,
    );

    let registry_name = matches.value_of("registry_name").unwrap().to_string();
    let image_name = matches.value_of("image_name").unwrap().to_string();
    let image_tag = matches
        .value_of("image_tag")
        .unwrap_or_default()
        .to_string();

    let mut bootstrap_validator = Validator::new(DockerImage::new(
        registry_name.clone(),
        ValidatorType::Bootstrap,
        image_name.clone(),
        image_tag.clone(),
        None,
    ));

    let mut validator = Validator::new(DockerImage::new(
        registry_name.clone(),
        ValidatorType::Standard,
        image_name.clone(),
        image_tag.clone(),
        None,
    ));

    let mut rpc_node = Validator::new(DockerImage::new(
        registry_name.clone(),
        ValidatorType::RPC,
        image_name.clone(),
        image_tag.clone(),
        None,
    ));

    let mut clients = vec![];
    let validators = vec![&bootstrap_validator, &validator, &rpc_node];
    if build_config.docker_build() {
        for v in &validators {
            match docker.build_image(&solana_root.get_root_path(), v.image()) {
                Ok(_) => info!("{} image built successfully", v.validator_type()),
                Err(err) => {
                    error!("Failed to build docker image {err}");
                    return;
                }
            }
        }

        for client_index in 0..client_config.num_clients {
            let client = Validator::new(DockerImage::new(
                registry_name.clone(),
                ValidatorType::Client,
                image_name.clone(),
                image_tag.clone(),
                Some(client_index),
            ));

            // need new image for each client since they each hold
            // their own client-accounts.yml
            match docker.build_client_image(
                &solana_root.get_root_path(),
                &client.image(),
                client_index,
            ) {
                Ok(_) => info!("Client image built successfully"),
                Err(err) => {
                    error!("Failed to build client image {err}");
                    return;
                }
            }

            clients.push(client);
        }

        match docker.push_images(&validators) {
            Ok(_) => info!("Validator images pushed successfully"),
            Err(err) => {
                error!("Failed to push Validator docker image {err}");
                return;
            }
        }

        match docker.push_client_images(&clients) {
            Ok(_) => info!("Client image pushed successfully"),
            Err(err) => {
                error!("Failed to push Client docker image {err}");
                return;
            }
        }
    }

    // metrics secret create once and use by all pods
    if kub_controller.metrics.is_some() {
        let metrics_secret = match kub_controller.create_metrics_secret() {
            Ok(secret) => secret,
            Err(err) => {
                error!("Failed to create metrics secret! {err}");
                return;
            }
        };
        match kub_controller.deploy_secret(&metrics_secret).await {
            Ok(_) => (),
            Err(err) => {
                error!("{err}");
                return;
            }
        }
    };

    match kub_controller.create_bootstrap_secret("bootstrap-accounts-secret", &config_directory) {
        Ok(secret) => bootstrap_validator.set_secret(secret),
        Err(err) => {
            error!("Failed to create bootstrap secret! {err}");
            return;
        }
    };

    match kub_controller
        .deploy_secret(bootstrap_validator.secret())
        .await
    {
        Ok(_) => info!("Deployed Bootstrap Secret"),
        Err(err) => {
            error!("{err}");
            return;
        }
    }

    // Create bootstrap labels
    let identity_path = config_directory.join("bootstrap-validator/identity.json");
    let bootstrap_keypair =
        read_keypair_file(identity_path).expect("Failed to read bootstrap keypair file");
    bootstrap_validator.add_label(
        "load-balancer/name",
        "load-balancer-selector",
        LabelType::ValidatorReplicaSet,
    );
    bootstrap_validator.add_label(
        "service/name",
        "bootstrap-validator-selector",
        LabelType::ValidatorReplicaSet,
    );
    bootstrap_validator.add_label(
        "validator/type",
        "bootstrap",
        LabelType::ValidatorReplicaSet,
    );
    bootstrap_validator.add_label(
        "validator/identity",
        bootstrap_keypair.pubkey().to_string(),
        LabelType::ValidatorReplicaSet,
    );

    // create bootstrap replica set
    match kub_controller.create_bootstrap_validator_replica_set(
        bootstrap_validator.image(),
        bootstrap_validator.secret().metadata.name.clone(),
        bootstrap_validator.replica_set_labels(),
    ) {
        Ok(replica_set) => bootstrap_validator.set_replica_set(replica_set),
        Err(err) => {
            error!("Error creating bootstrap validator replicas_set: {err}");
            return;
        }
    };

    match kub_controller
        .deploy_replicas_set(bootstrap_validator.replica_set())
        .await
    {
        Ok(_) => {
            info!(
                "{} deployed successfully",
                bootstrap_validator.replica_set_name()
            );
        }
        Err(err) => {
            error!("Error! Failed to deploy bootstrap validator replicas_set. err: {err}");
            return;
        }
    };

    bootstrap_validator.add_label(
        "service/name",
        "bootstrap-validator-selector",
        LabelType::ValidatorService,
    );

    let bootstrap_service = kub_controller.create_service(
        "bootstrap-validator-service",
        bootstrap_validator.service_labels(),
    );
    match kub_controller.deploy_service(&bootstrap_service).await {
        Ok(_) => info!("bootstrap validator service deployed successfully"),
        Err(err) => error!("Error! Failed to deploy bootstrap validator service. err: {err}"),
    }

    //load balancer service. only create one and use for all deployments
    let load_balancer_label =
        kub_controller.create_selector("load-balancer/name", "load-balancer-selector");
    //create load balancer
    let load_balancer = kub_controller
        .create_validator_load_balancer("bootstrap-and-rpc-lb-service", &load_balancer_label);

    //deploy load balancer
    match kub_controller.deploy_service(&load_balancer).await {
        Ok(_) => info!("load balancer service deployed successfully"),
        Err(err) => error!("Error! Failed to deploy load balancer service. err: {err}"),
    }

    // wait for bootstrap replicaset to deploy
    while {
        match kub_controller
            .check_replica_set_ready(bootstrap_validator.replica_set_name().as_str())
            .await
        {
            Ok(ok) => !ok, // Continue the loop if replica set is not ready: Ok(false)
            Err(err) => panic!("Error occurred while checking replica set readiness: {err}"),
        }
    } {
        info!("{} not ready...", bootstrap_validator.replica_set_name());
        thread::sleep(Duration::from_secs(1));
    }

    if num_rpc_nodes > 0 {
        let mut rpc_nodes = vec![];
        for rpc_index in 0..num_rpc_nodes {
            match kub_controller.create_rpc_secret(rpc_index, &config_directory) {
                Ok(secret) => rpc_node.set_secret(secret),
                Err(err) => {
                    error!("Failed to create RPC node {rpc_index} secret! {err}");
                    return;
                }
            }
            match kub_controller.deploy_secret(&rpc_node.secret()).await {
                Ok(_) => info!("Deployed RPC node {rpc_index} Secret"),
                Err(err) => {
                    error!("{err}");
                    return;
                }
            }
            let identity_path =
                config_directory.join(format!("rpc-node-identity-{rpc_index}.json"));
            let rpc_keypair =
                read_keypair_file(identity_path).expect("Failed to read rpc-node keypair file");

            rpc_node.add_label(
                "rpc-node/name",
                &format!("rpc-node-{rpc_index}"),
                LabelType::ValidatorReplicaSet,
            );

            rpc_node.add_label(
                "rpc-node/type",
                rpc_node.validator_type().to_string(),
                LabelType::ValidatorReplicaSet,
            );

            rpc_node.add_label(
                "rpc-node/identity",
                rpc_keypair.pubkey().to_string(),
                LabelType::ValidatorReplicaSet,
            );

            rpc_node.add_label(
                "load-balancer/name",
                "load-balancer-selector",
                LabelType::ValidatorReplicaSet,
            );

            let rpc_replica_set = match kub_controller.create_rpc_replica_set(
                rpc_node.image(),
                rpc_node.secret().metadata.name.clone(),
                rpc_node.replica_set_labels(),
                rpc_index,
            ) {
                Ok(replica_set) => replica_set,
                Err(err) => {
                    error!("Error creating rpc node replicas_set: {err}");
                    return;
                }
            };

            // deploy rpc node replica set
            let rpc_node_name = match kub_controller.deploy_replicas_set(&rpc_replica_set).await {
                Ok(rs) => {
                    info!("rpc node replica set ({rpc_index}) deployed successfully");
                    rs.metadata.name.unwrap()
                }
                Err(err) => {
                    error!("Error! Failed to deploy rpc node replica set: {rpc_index}. err: {err}");
                    return;
                }
            };
            rpc_nodes.push(rpc_node_name);

            rpc_node.add_label(
                "service/name",
                &format!("rpc-node-selector-{rpc_index}"),
                LabelType::ValidatorService,
            );

            let rpc_service = kub_controller.create_service(
                format!("rpc-node-selector-{rpc_index}").as_str(),
                rpc_node.service_labels(),
            );
            match kub_controller.deploy_service(&rpc_service).await {
                Ok(_) => info!("rpc node service deployed successfully"),
                Err(err) => error!("Error! Failed to deploy rpc node service. err: {err}"),
            }
        }

        // wait for at least one rpc node to deploy
        loop {
            let mut one_rpc_node_ready = false;
            for rpc_node in &rpc_nodes {
                match kub_controller
                    .check_replica_set_ready(rpc_node.as_str())
                    .await
                {
                    Ok(ready) => {
                        if ready {
                            one_rpc_node_ready = true;
                            break;
                        }
                    } // Continue the loop if replica set is not ready: Ok(false)
                    Err(err) => panic!(
                        "Error occurred while checking rpc node replica set readiness: {err}"
                    ),
                }
            }

            if one_rpc_node_ready {
                break;
            }

            info!("no rpc replica sets ready yet");
            thread::sleep(Duration::from_secs(10));
        }
        info!(">= 1 rpc node ready");
    }

    // Create and deploy validators secrets/selectors
    for validator_index in 0..num_validators {
        match kub_controller.create_validator_secret(validator_index, &config_directory) {
            Ok(secret) => validator.set_secret(secret),
            Err(err) => {
                error!("Failed to create validator secret! {err}");
                return;
            }
        };

        match kub_controller.deploy_secret(&validator.secret()).await {
            Ok(_) => info!("Deployed validator {validator_index} secret"),
            Err(err) => {
                error!("{err}");
                return;
            }
        }

        let identity_path =
            config_directory.join(format!("validator-identity-{validator_index}.json"));
        let validator_keypair =
            read_keypair_file(identity_path).expect("Failed to read validator keypair file");

        validator.add_label(
            "validator/name",
            &format!("validator-{validator_index}"),
            LabelType::ValidatorReplicaSet,
        );
        validator.add_label(
            "validator/type",
            validator.validator_type().to_string(),
            LabelType::ValidatorReplicaSet,
        );
        validator.add_label(
            "validator/identity",
            validator_keypair.pubkey().to_string(),
            LabelType::ValidatorReplicaSet,
        );

        let validator_replica_set = match kub_controller.create_validator_replica_set(
            validator.image(),
            validator.secret().metadata.name.clone(),
            validator.replica_set_labels(),
            validator_index,
        ) {
            Ok(replica_set) => replica_set,
            Err(err) => {
                error!("Error creating validator replicas_set: {err}");
                return;
            }
        };

        let _ = match kub_controller
            .deploy_replicas_set(&validator_replica_set)
            .await
        {
            Ok(rs) => {
                info!("validator replica set ({validator_index}) deployed successfully");
                rs.metadata.name.unwrap()
            }
            Err(err) => {
                error!(
                    "Error! Failed to deploy validator replica set: {validator_index}. err: {err}"
                );
                return;
            }
        };

        let validator_service = kub_controller.create_service(
            &format!("validator-service-{validator_index}"),
            validator.replica_set_labels(),
        );
        match kub_controller.deploy_service(&validator_service).await {
            Ok(_) => info!("validator service ({validator_index}) deployed successfully"),
            Err(err) => {
                error!("Error! Failed to deploy validator service: {validator_index}. err: {err}")
            }
        }
    }

    if client_config.num_clients <= 0 {
        return;
    }

    for (client_index, client) in clients.iter_mut().enumerate() {
        match kub_controller.create_client_secret(client_index, &config_directory) {
            Ok(secret) => client.set_secret(secret),
            Err(err) => {
                error!("Failed to create Client {client_index} secret! {err}");
                return;
            }
        }
        match kub_controller.deploy_secret(&client.secret()).await {
            Ok(_) => info!("Deployed Client {client_index} Secret"),
            Err(err) => {
                error!("{err}");
                return;
            }
        }

        client.add_label(
            "client/name",
            format!("client-{}", client_index),
            LabelType::ValidatorReplicaSet,
        );

        let client_replica_set = match kub_controller.create_client_replica_set(
            client.image(),
            client.secret().metadata.name.clone(),
            client.replica_set_labels(),
            client_index,
        ) {
            Ok(replica_set) => replica_set,
            Err(err) => {
                error!("Error creating client replicas_set: {err}");
                return;
            }
        };

        let _ = match kub_controller
            .deploy_replicas_set(&client_replica_set)
            .await
        {
            Ok(rs) => {
                info!("client replica set ({client_index}) deployed successfully",);
                rs.metadata.name.unwrap()
            }
            Err(err) => {
                error!("Error! Failed to deploy client replica set: {client_index}. err: {err}");
                return;
            }
        };

        let client_service = kub_controller.create_service(
            &format!("client-service-{client_index}"),
            client.replica_set_labels(),
        );
        match kub_controller.deploy_service(&client_service).await {
            Ok(_) => info!("client service ({client_index}) deployed successfully"),
            Err(err) => {
                error!("Error! Failed to deploy client service: {client_index}. err: {err}")
            }
        }
    }
}
