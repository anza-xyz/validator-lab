use {
    clap::{command, value_t_or_exit, Arg, ArgGroup},
    log::*,
    solana_ledger::blockstore_cleanup_service::{
        DEFAULT_MAX_LEDGER_SHREDS, DEFAULT_MIN_MAX_LEDGER_SHREDS,
    },
    solana_sdk::{signature::keypair::read_keypair_file, signer::Signer},
    std::fs,
    strum::VariantNames,
    validator_lab::{
        docker::{DockerConfig, DockerImage},
        genesis::{Genesis, GenesisFlags},
        kubernetes::{Kubernetes, PodRequests},
        release::{BuildConfig, BuildType, DeployMethod},
        validator::Validator,
        validator_config::ValidatorConfig,
        SolanaRoot, ValidatorType,
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
                .requires("image_name")
                .requires("base_image")
                .requires("image_tag")
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
                .required(true)
                .help("Docker image name. Will be prepended with validator_type (bootstrap or validator)"),
        )
        .arg(
            Arg::with_name("base_image")
                .long("base-image")
                .takes_value(true)
                .default_value("ubuntu:20.04")
                .required(true)
                .help("Docker base image"),
        )
        .arg(
            Arg::with_name("image_tag")
                .long("tag")
                .takes_value(true)
                .required(true)
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

    let mut kub_controller = Kubernetes::new(
        environment_config.namespace,
        &mut validator_config,
        pod_requests,
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

    // creates genesis and writes to binary file
    match genesis.generate(solana_root.get_root_path(), build_config.build_path()) {
        Ok(_) => (),
        Err(err) => {
            error!("generate genesis error! {}", err);
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

    let mut bootstrap_validator = Validator::new(DockerImage::new(
        matches.value_of("registry_name").unwrap().to_string(),
        ValidatorType::Bootstrap,
        matches.value_of("image_name").unwrap().to_string(),
        matches
            .value_of("image_tag")
            .unwrap_or_default()
            .to_string(),
    ));

    if build_config.docker_build() {
        match docker.build_image(solana_root.get_root_path(), bootstrap_validator.image()) {
            Ok(_) => info!(
                "{} image built successfully",
                bootstrap_validator.validator_type()
            ),
            Err(err) => {
                error!("Exiting........ {err}");
                return;
            }
        }

        match DockerConfig::push_image(bootstrap_validator.image()) {
            Ok(_) => info!(
                "{} image pushed successfully",
                bootstrap_validator.validator_type()
            ),
            Err(err) => {
                error!("Exiting........ {err}");
                return;
            }
        }
    }

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
    bootstrap_validator.add_label("validator/lb", "load-balancer-selector");
    bootstrap_validator.add_label("validator/name", "bootstrap-validator-selector");
    bootstrap_validator.add_label("validator/type", "bootstrap");
    bootstrap_validator.add_label("validator/identity", bootstrap_keypair.pubkey().to_string());

    // create bootstrap replica set
    match kub_controller.create_bootstrap_validator_replica_set(
        bootstrap_validator.image(),
        bootstrap_validator.secret().metadata.name.clone(),
        bootstrap_validator.labels(),
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
}
