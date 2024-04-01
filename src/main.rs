use {
    clap::{command, Arg, ArgGroup},
    log::*,
    solana_sdk::{signature::keypair::read_keypair_file, signer::Signer},
    std::fs,
    strum::VariantNames,
    validator_lab::{
        docker::{DockerConfig, DockerImage},
        genesis::{Genesis, GenesisFlags},
        kubernetes::Kubernetes,
        release::{BuildConfig, BuildType, DeployMethod},
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
                .possible_values(["true", "false"])
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

    let (solana_root, build_path) = match &deploy_method {
        DeployMethod::Local(path) => {
            let root = SolanaRoot::new_from_path(path.into());
            let path = root.get_root_path().join("farf/bin");
            (root, path)
        }
        DeployMethod::ReleaseChannel(_) => {
            let root = SolanaRoot::default();
            let path = root.get_root_path().join("solana-release/bin");
            (root, path)
        }
    };

    let build_type: BuildType = matches.value_of_t("build_type").unwrap();

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

    let kub_controller = Kubernetes::new(environment_config.namespace).await;
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
        Ok(_) => info!("Generated faucet account"),
        Err(err) => {
            error!("generate faucet error! {err}");
            return;
        }
    }

    match genesis.generate_accounts(ValidatorType::Bootstrap, 1) {
        Ok(_) => info!("Generated bootstrap account"),
        Err(err) => {
            error!("generate accounts error! {err}");
            return;
        }
    }

    // creates genesis and writes to binary file
    match genesis.generate(solana_root.get_root_path(), &build_path) {
        Ok(_) => info!("Created genesis successfully"),
        Err(err) => {
            error!("generate genesis error! {err}");
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

    let validator_type = ValidatorType::Bootstrap;
    let docker_image = DockerImage::new(
        matches.value_of("registry_name").unwrap().to_string(),
        validator_type,
        matches.value_of("image_name").unwrap().to_string(),
        matches
            .value_of("image_tag")
            .unwrap_or_default()
            .to_string(),
    );

    if build_config.docker_build() {
        match docker.build_image(solana_root.get_root_path(), &docker_image) {
            Ok(_) => info!("{} image built successfully", docker_image.validator_type()),
            Err(err) => {
                error!("Exiting........ {err}");
                return;
            }
        }

        match DockerConfig::push_image(&docker_image) {
            Ok(_) => info!(
                "{} image pushed successfully",
                docker_image.validator_type()
            ),
            Err(err) => {
                error!("Error. Failed to build imge: {err}");
                return;
            }
        }
    }

    let bootstrap_secret = match kub_controller
        .create_bootstrap_secret("bootstrap-accounts-secret", &config_directory)
    {
        Ok(secret) => secret,
        Err(err) => {
            error!("Failed to create bootstrap secret! {}", err);
            return;
        }
    };

    match kub_controller.deploy_secret(&bootstrap_secret).await {
        Ok(_) => info!("Deployed Bootstrap Secret"),
        Err(err) => {
            error!("{}", err);
            return;
        }
    }

    // Bootstrap needs two labels. Because it is going to have two services.
    // One via Load Balancer, one direct
    let mut bootstrap_rs_labels =
        kub_controller.create_selector("validator/lb", "load-balancer-selector");
    bootstrap_rs_labels.insert(
        "validator/name".to_string(),
        "bootstrap-validator-selector".to_string(),
    );
    bootstrap_rs_labels.insert("validator/type".to_string(), "bootstrap".to_string());

    let identity_path = config_directory.join("bootstrap-validator/identity.json");
    let bootstrap_keypair =
        read_keypair_file(identity_path).expect("Failed to read bootstrap keypair file");
    bootstrap_rs_labels.insert(
        "validator/identity".to_string(),
        bootstrap_keypair.pubkey().to_string(),
    );

    let bootstrap_replica_set = match kub_controller.create_bootstrap_validator_replica_set(
        bootstrap_container_name,
        bootstrap_image_name,
        bootstrap_secret.metadata.name.clone(),
        &bootstrap_rs_labels,
    ) {
        Ok(replica_set) => replica_set,
        Err(err) => {
            error!("Error creating bootstrap validator replicas_set: {}", err);
            return;
        }
    };
}
