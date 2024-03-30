use {
    clap::{command, Arg},
    log::*,
    std::fs,
    validator_lab::{
        kubernetes::Kubernetes,
        release::{BuildConfig, DeployMethod},
        SolanaRoot,
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
            Arg::with_name("skip_build")
                .long("skip-build")
                .help("Disable building for building from local repo"),
        )
        .arg(
            Arg::with_name("debug_build")
                .long("debug-build")
                .help("Enable debug build"),
        )
        .arg(
            Arg::with_name("release_channel")
                .long("release-channel")
                .takes_value(true)
                .conflicts_with("local_path")
                .help("Pulls specific release version. e.g. v1.17.2"),
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
        DeployMethod::Skip
    };

    let solana_root = match &deploy_method {
        DeployMethod::Local(path) => SolanaRoot::new_from_path(path.into()),
        DeployMethod::ReleaseChannel(_) | DeployMethod::Skip => SolanaRoot::default(),
    };

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
            error!("Error: {}", err);
            return;
        }
    }

    let build_config = BuildConfig::new(
        deploy_method,
        matches.is_present("skip_build"),
        matches.is_present("debug_build"),
        &solana_root.get_root_path(),
    )
    .unwrap_or_else(|err| {
        panic!("Error creating BuildConfig: {}", err);
    });

    match build_config.prepare().await {
        Ok(_) => info!("Validator setup prepared successfully"),
        Err(err) => {
            error!("Error: {}", err);
            return;
        }
    }
}
