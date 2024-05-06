use {
    clap::{command, value_t_or_exit, Arg, ArgGroup},
    log::*,
    solana_ledger::blockstore_cleanup_service::{
        DEFAULT_MAX_LEDGER_SHREDS, DEFAULT_MIN_MAX_LEDGER_SHREDS,
    },
    solana_sdk::{signature::keypair::read_keypair_file, signer::Signer},
    std::{fs, path::PathBuf, result::Result},
    strum::VariantNames,
    validator_lab::{
        cluster_images::ClusterImages,
        docker::{DockerConfig, DockerImage},
        genesis::{
            Genesis, GenesisFlags, DEFAULT_BOOTSTRAP_NODE_SOL, DEFAULT_BOOTSTRAP_NODE_STAKE_SOL,
            DEFAULT_FAUCET_LAMPORTS, DEFAULT_INTERNAL_NODE_SOL, DEFAULT_INTERNAL_NODE_STAKE_SOL,
            DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
        },
        kubernetes::{Kubernetes, PodRequests},
        ledger_helper::LedgerHelper,
        release::{BuildConfig, BuildType, DeployMethod},
        validator::{LabelType, Validator},
        validator_config::ValidatorConfig,
        EnvironmentConfig, Metrics, SolanaRoot, ValidatorType,
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
                .requires("build_directory")
                .help("Pulls specific release version. e.g. v1.17.2"),
        )
        .group(
            ArgGroup::new("required_group")
                .args(&["local_path", "release_channel"])
                .required(true),
        )
        .arg(
            Arg::with_name("build_directory")
                .long("build-dir")
                .takes_value(true)
                .conflicts_with("local_path")
                .help("Absolute path to build directory for release-channel
                e.g. /home/sol/validator-lab-build"),
        )
        // non-bootstrap validators
        .arg(
            Arg::with_name("number_of_validators")
                .long("num-validators")
                .takes_value(true)
                .default_value("0")
                .help("Number of validators to deploy")
                .validator(|s| match s.parse::<i32>() {
                    Ok(n) if n >= 0 => Ok(()),
                    _ => Err(String::from("number_of_validators should be >= 0")),
                }),
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
                .help("override the number of slots in an epoch. Default for cluster_type: development -> 8192.
                Default for cluster_type: devnet, testnet, mainnet-beta -> 432000 (1 epoch every ~= 2 days)"),
        )
        .arg(
            Arg::with_name("target_lamports_per_signature")
                .long("target-lamports-per-signature")
                .takes_value(true)
                .help("Genesis config. target lamports per signature. Default: 10000"),
        )
        .arg(
            Arg::with_name("faucet_lamports")
                .long("faucet-lamports")
                .takes_value(true)
                .default_value(&DEFAULT_FAUCET_LAMPORTS.to_string())
                .help("Override the default 500000000000000000 lamports minted in genesis"),
        )
        .arg(
            Arg::with_name("enable_warmup_epochs")
                .long("enable-warmup-epochs")
                .help("Genesis config. enable warmup epoch. defaults to true"),
        )
        .arg(
            Arg::with_name("max_genesis_archive_unpacked_size")
                .long("max-genesis-archive-unpacked-size")
                .takes_value(true)
                .default_value(&DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE.to_string())
                .help("Genesis config. max_genesis_archive_unpacked_size"),
        )
        .arg(
            Arg::with_name("cluster_type")
                .long("cluster-type")
                .possible_values(["development", "devnet", "testnet", "mainnet-beta"])
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
                .default_value(&DEFAULT_BOOTSTRAP_NODE_SOL.to_string())
                .help("Genesis config. bootstrap validator sol"),
        )
        .arg(
            Arg::with_name("bootstrap_validator_stake_sol")
                .long("bootstrap-validator-stake-sol")
                .takes_value(true)
                .default_value(&DEFAULT_BOOTSTRAP_NODE_STAKE_SOL.to_string())
                .help("Genesis config. bootstrap validator stake sol"),
        )
        //Docker config
        .arg(
            Arg::with_name("skip_docker_build")
                .long("skip-docker-build")
                .help("Skips build Docker images"),
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
            Arg::with_name("limit_ledger_size")
                .long("limit-ledger-size")
                .takes_value(true)
                .default_value(&DEFAULT_MAX_LEDGER_SHREDS.to_string())
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
                .default_value(&DEFAULT_INTERNAL_NODE_SOL.to_string())
                .help("Amount to fund internal nodes in genesis config."),
        )
        .arg(
            Arg::with_name("internal_node_stake_sol")
                .long("internal-node-stake-sol")
                .takes_value(true)
                .default_value(&DEFAULT_INTERNAL_NODE_STAKE_SOL.to_string())
                .help("Amount to stake internal nodes (Sol)."),
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
                .default_value("70Gi") // 70 Gibibytes
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "INFO");
    }
    solana_logger::setup();
    let matches = parse_matches();
    let environment_config = EnvironmentConfig {
        namespace: matches.value_of("cluster_namespace").unwrap_or_default(),
        build_directory: matches.value_of("build_directory").map(PathBuf::from),
    };

    let num_validators = value_t_or_exit!(matches, "number_of_validators", usize);

    let deploy_method = if let Some(local_path) = matches.value_of("local_path") {
        DeployMethod::Local(local_path.to_owned())
    } else if let Some(release_channel) = matches.value_of("release_channel") {
        DeployMethod::ReleaseChannel(release_channel.to_owned())
    } else {
        unreachable!("One of --local-path or --release-channel must be provided.");
    };

    let (solana_root, build_path) = match &deploy_method {
        DeployMethod::Local(path) => {
            let root = SolanaRoot::new_from_path(path.into());
            let path = root.get_root_path().join("farf/bin");
            (root, path)
        }
        DeployMethod::ReleaseChannel(_) => {
            // unwrap safe since required if release-channel used
            let root =
                SolanaRoot::new_from_path(environment_config.build_directory.unwrap().clone());
            let path = root.get_root_path().join("solana-release/bin");
            (root, path)
        }
    };

    let build_type: BuildType = matches.value_of_t("build_type").unwrap();

    if let Ok(metadata) = fs::metadata(solana_root.get_root_path()) {
        if !metadata.is_dir() {
            return Err(format!(
                "Build path is not a directory: {}",
                solana_root.get_root_path().display()
            )
            .into());
        }
    } else {
        return Err(format!(
            "Build directory not found: {}",
            solana_root.get_root_path().display()
        )
        .into());
    }

    let build_config = BuildConfig::new(
        deploy_method.clone(),
        build_type,
        solana_root.get_root_path(),
        !matches.is_present("skip_docker_build"),
    );

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
        enable_warmup_epochs: matches.is_present("enable_warmup_epochs"),
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

    let limit_ledger_size = value_t_or_exit!(matches, "limit_ledger_size", u64);
    let mut validator_config = ValidatorConfig {
        internal_node_sol: value_t_or_exit!(matches, "internal_node_sol", f64),
        internal_node_stake_sol: value_t_or_exit!(matches, "internal_node_stake_sol", f64),
        shred_version: None, // set after genesis created
        max_ledger_size: if limit_ledger_size < DEFAULT_MIN_MAX_LEDGER_SHREDS {
            clap::Error::with_description(
                    format!("The provided --limit-ledger-size value was too small, the minimum value is {DEFAULT_MIN_MAX_LEDGER_SHREDS}"),
                    clap::ErrorKind::ArgumentNotFound,
                )
                .exit();
        } else {
            Some(limit_ledger_size)
        },
        skip_poh_verify: matches.is_present("skip_poh_verify"),
        no_snapshot_fetch: matches.is_present("no_snapshot_fetch"),
        require_tower: matches.is_present("require_tower"),
        enable_full_rpc: matches.is_present("enable_full_rpc"),
        known_validators: vec![],
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
    )
    .await;

    let exists = kub_controller.namespace_exists().await?;
    if !exists {
        return Err(format!(
            "Namespace: '{}' doesn't exist. Exiting...",
            environment_config.namespace
        )
        .into());
    }

    build_config.prepare().await?;
    info!("Validator setup prepared successfully");

    let config_directory = solana_root.get_root_path().join("config-k8s");
    let mut genesis = Genesis::new(config_directory.clone(), genesis_flags);

    genesis.generate_faucet()?;
    info!("Generated faucet account");

    genesis.generate_accounts(ValidatorType::Bootstrap, 1)?;
    info!("Generated bootstrap account");

    // creates genesis and writes to binary file
    genesis
        .generate(solana_root.get_root_path(), &build_path)
        .await?;
    info!("Created genesis successfully");

    // generate standard validator accounts
    genesis.generate_accounts(ValidatorType::Standard, num_validators)?;
    info!("Generated {num_validators} validator account(s)");

    let ledger_dir = config_directory.join("bootstrap-validator");
    let shred_version = LedgerHelper::get_shred_version(&ledger_dir)?;
    kub_controller.set_shred_version(shred_version);

    //unwraps are safe here. since their requirement is enforced by argmatches
    let docker = DockerConfig::new(
        matches.value_of("base_image").unwrap().to_string(),
        deploy_method,
    );

    let registry_name = matches.value_of("registry_name").unwrap().to_string();
    let image_name = matches.value_of("image_name").unwrap().to_string();
    let image_tag = matches
        .value_of("image_tag")
        .unwrap_or_default()
        .to_string();

    let mut cluster_images = ClusterImages::default();

    let bootstrap_validator = Validator::new(DockerImage::new(
        registry_name.clone(),
        ValidatorType::Bootstrap,
        image_name.clone(),
        image_tag.clone(),
    ));
    cluster_images.set_item(bootstrap_validator, ValidatorType::Bootstrap);

    if num_validators > 0 {
        let validator = Validator::new(DockerImage::new(
            registry_name.clone(),
            ValidatorType::Standard,
            image_name.clone(),
            image_tag.clone(),
        ));
        cluster_images.set_item(validator, ValidatorType::Standard);
    }

    if build_config.docker_build() {
        for v in cluster_images.get_validators() {
            docker.build_image(solana_root.get_root_path(), v.image())?;
            info!("{} image built successfully", v.validator_type());
        }

        docker.push_images(cluster_images.get_validators())?;
        info!("Validator images pushed successfully");
    }

    // metrics secret create once and use by all pods
    if kub_controller.metrics.is_some() {
        let metrics_secret = kub_controller.create_metrics_secret()?;
        kub_controller.deploy_secret(&metrics_secret).await?;
    };

    let bootstrap_validator = cluster_images.bootstrap()?;
    let secret =
        kub_controller.create_bootstrap_secret("bootstrap-accounts-secret", &config_directory)?;
    bootstrap_validator.set_secret(secret);

    kub_controller
        .deploy_secret(bootstrap_validator.secret())
        .await?;
    info!("Deployed Bootstrap Secret");

    // Create Bootstrap labels
    // Bootstrap needs two labels, one for each service.
    // One for Load Balancer, one direct
    let identity_path = config_directory.join("bootstrap-validator/identity.json");
    let bootstrap_keypair =
        read_keypair_file(identity_path).expect("Failed to read bootstrap keypair file");
    kub_controller.add_known_validator(bootstrap_keypair.pubkey());

    bootstrap_validator.add_label(
        "load-balancer/name",
        "load-balancer-selector",
        LabelType::Service,
    );
    bootstrap_validator.add_label(
        "service/name",
        "bootstrap-validator-selector",
        LabelType::Service,
    );
    bootstrap_validator.add_label(
        "validator/type",
        bootstrap_validator.validator_type().to_string(),
        LabelType::Info,
    );
    bootstrap_validator.add_label(
        "validator/identity",
        bootstrap_keypair.pubkey().to_string(),
        LabelType::Info,
    );

    // create bootstrap replica set
    let replica_set = kub_controller.create_bootstrap_validator_replica_set(
        bootstrap_validator.image(),
        bootstrap_validator.secret().metadata.name.clone(),
        &bootstrap_validator.all_labels(),
    )?;
    bootstrap_validator.set_replica_set(replica_set);

    // deploy bootstrap replica set
    kub_controller
        .deploy_replicas_set(bootstrap_validator.replica_set())
        .await?;
    info!(
        "{} deployed successfully",
        bootstrap_validator.replica_set_name()
    );

    // create and deploy bootstrap-service
    let bootstrap_service = kub_controller.create_service(
        "bootstrap-validator-service",
        bootstrap_validator.service_labels(),
    );
    kub_controller.deploy_service(&bootstrap_service).await?;
    info!("bootstrap validator service deployed successfully");

    // load balancer service. only create one and use for all bootstrap/rpc nodes
    // service selector matches bootstrap selector
    let load_balancer_label =
        kub_controller.create_selector("load-balancer/name", "load-balancer-selector");
    //create load balancer
    let load_balancer = kub_controller
        .create_validator_load_balancer("bootstrap-and-rpc-node-lb-service", &load_balancer_label);

    //deploy load balancer
    kub_controller.deploy_service(&load_balancer).await?;
    info!("load balancer service deployed successfully");

    // wait for bootstrap replicaset to deploy
    while !kub_controller
        .is_replica_set_ready(bootstrap_validator.replica_set_name().as_str())
        .await?
    {
        info!(
            "replica set: {} not ready...",
            bootstrap_validator.replica_set_name()
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    if num_validators == 0 {
        info!("No validators to deploy. Returning");
        return Ok(());
    }
    let validator = cluster_images.validator()?;

    for validator_index in 0..num_validators {
        // Create and deploy validators secrets
        let validator_secret =
            kub_controller.create_validator_secret(validator_index, &config_directory)?;
        validator.set_secret(validator_secret);
        kub_controller.deploy_secret(validator.secret()).await?;
        info!("Deployed Validator {validator_index} secret");

        let identity_path =
            config_directory.join(format!("validator-identity-{validator_index}.json"));
        let validator_keypair =
            read_keypair_file(identity_path).expect("Failed to read validator keypair file");

        validator.add_label(
            "validator/name",
            &format!("validator-{validator_index}"),
            LabelType::Service,
        );
        validator.add_label(
            "validator/type",
            validator.validator_type().to_string(),
            LabelType::Info,
        );
        validator.add_label(
            "validator/identity",
            validator_keypair.pubkey().to_string(),
            LabelType::Info,
        );

        let replica_set = kub_controller.create_validator_replica_set(
            validator.image(),
            validator.secret().metadata.name.clone(),
            &validator.all_labels(),
            validator_index,
        )?;
        validator.set_replica_set(replica_set);

        kub_controller
            .deploy_replicas_set(validator.replica_set())
            .await?;
        info!("validator replica set ({validator_index}) deployed successfully");

        let validator_service = kub_controller.create_service(
            &format!("validator-service-{validator_index}"),
            validator.service_labels(),
        );
        kub_controller.deploy_service(&validator_service).await?;
        info!("validator service ({validator_index}) deployed successfully");
    }

    Ok(())
}
