use {
    crate::{new_spinner_progress_bar, release::DeployMethod, ValidatorType, SUN, WRITING, add_tag_to_name},
    anyhow::Error as AnyHowError,
    log::*,
    rand::Rng,
    rayon::prelude::*,
    solana_core::gen_keys::GenKeys,
    solana_sdk::{
        native_token::sol_to_lamports,
        signature::{write_keypair_file, Keypair},
    },
    std::{
        error::Error,
        fs::{File, OpenOptions},
        io::{self, BufRead, BufWriter, Read, Write},
        path::PathBuf,
        process::Command,
        result::Result,
    },
};

pub const DEFAULT_FAUCET_LAMPORTS: u64 = 500000000000000000; // from agave/
pub const DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE: u64 = 1073741824; // from agave/
pub const DEFAULT_INTERNAL_NODE_STAKE_SOL: f64 = 10.0;
pub const DEFAULT_INTERNAL_NODE_SOL: f64 = 100.0;
pub const DEFAULT_BOOTSTRAP_NODE_STAKE_SOL: f64 = 10.0;
pub const DEFAULT_BOOTSTRAP_NODE_SOL: f64 = 100.0;
pub const DEFAULT_CLIENT_LAMPORTS_PER_SIGNATURE: u64 = 42;

fn fetch_spl(fetch_spl_file: &PathBuf) -> Result<(), Box<dyn Error>> {
    let output = Command::new("bash")
        .arg(fetch_spl_file)
        .output() // Capture the output of the script
        .expect("Failed to run fetch-spl.sh script");

    // Check if the script execution was successful
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Failed to fun fetch-spl.sh script: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn parse_spl_genesis_file(spl_file: &PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
    // Read entire file into a String
    let mut file = File::open(spl_file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Split by whitespace
    let mut args = Vec::new();
    let mut tokens_iter = content.split_whitespace();

    while let Some(token) = tokens_iter.next() {
        args.push(token.to_string());
        // Find flag delimiters
        if token.starts_with("--") {
            for next_token in tokens_iter.by_ref() {
                if next_token.starts_with("--") {
                    args.push(next_token.to_string());
                } else {
                    args.push(next_token.to_string());
                    break;
                }
            }
        }
    }

    Ok(args)
}

fn append_client_accounts_to_file(
    in_file: &PathBuf,  //bench-tps-i.yml
    out_file: &PathBuf, //client-accounts.yml
) -> io::Result<()> {
    // Open the bench-tps-i.yml file for reading.
    let input = File::open(in_file)?;
    let reader = io::BufReader::new(input);

    // Open (or create) client-accounts.yml
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(out_file)?;
    let mut writer = BufWriter::new(output);

    // Enumerate the lines of the input file, starting from 1.
    for (index, line) in reader.lines().enumerate().map(|(i, l)| (i + 1, l)) {
        let line = line?;

        // Skip first line since it is a header aka "---" in a yaml
        if (index as u64) > 1 {
            writeln!(writer, "{line}")?;
        }
    }

    Ok(())
}

pub struct GenesisFlags {
    pub hashes_per_tick: String,
    pub slots_per_epoch: Option<u64>,
    pub target_lamports_per_signature: Option<u64>,
    pub faucet_lamports: Option<u64>,
    pub enable_warmup_epochs: bool,
    pub max_genesis_archive_unpacked_size: Option<u64>,
    pub cluster_type: String,
    pub bootstrap_validator_sol: Option<f64>,
    pub bootstrap_validator_stake_sol: Option<f64>,
}

impl std::fmt::Display for GenesisFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "GenesisFlags {{\n\
             hashes_per_tick: {:?},\n\
             slots_per_epoch: {:?},\n\
             target_lamports_per_signature: {:?},\n\
             faucet_lamports: {:?},\n\
             enable_warmup_epochs: {},\n\
             max_genesis_archive_unpacked_size: {:?},\n\
             cluster_type: {}\n\
             bootstrap_validator_sol: {:?},\n\
             bootstrap_validator_stake_sol: {:?},\n\
             }}",
            self.hashes_per_tick,
            self.slots_per_epoch,
            self.target_lamports_per_signature,
            self.faucet_lamports,
            self.enable_warmup_epochs,
            self.max_genesis_archive_unpacked_size,
            self.cluster_type,
            self.bootstrap_validator_sol,
            self.bootstrap_validator_stake_sol,
        )
    }
}

pub struct Genesis {
    config_dir: PathBuf,
    key_generator: GenKeys,
    pub flags: GenesisFlags,
}

impl Genesis {
    pub fn new(config_dir: PathBuf, flags: GenesisFlags, retain_previous_genesis_directory: bool) -> Self {
        if !retain_previous_genesis_directory {
            if config_dir.exists() {
                std::fs::remove_dir_all(&config_dir).unwrap();
            }
            std::fs::create_dir_all(&config_dir).unwrap();
        }

        let seed: [u8; 32] = rand::thread_rng().gen();

        Self {
            config_dir,
            key_generator: GenKeys::new(seed),
            flags,
        }
    }

    pub fn generate_faucet(&mut self) -> Result<(), Box<dyn Error>> {
        info!("generating faucet keypair");
        let outfile = self.config_dir.join("faucet.json");
        let keypair = self.key_generator.gen_keypair();

        write_keypair_file(&keypair, outfile)?;
        Ok(())
    }

    pub fn generate_accounts(
        &mut self,
        validator_type: ValidatorType,
        number_of_accounts: usize,
        deployment_tag: Option<String>,
    ) -> Result<(), Box<dyn Error>> {
        if number_of_accounts == 0 {
            return Ok(());
        }
        if validator_type == ValidatorType::Client {
            return Err("Client valdiator_type in generate_accounts not allowed".into());
        }

        info!(
            "generating {} {} account(s)...",
            number_of_accounts, validator_type
        );

        let mut account_types = vec!["identity", "stake-account", "vote-account"];
        match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard => (),
            ValidatorType::RPC => {
                account_types.pop(); // no vote-account for RPC
            }
            ValidatorType::Client => panic!("Client type not supported"),
        };

        let total_accounts_to_generate = number_of_accounts * account_types.len();
        let keypairs = self
            .key_generator
            .gen_n_keypairs(total_accounts_to_generate as u64);

        self.write_accounts_to_file(validator_type, account_types, keypairs, deployment_tag)?;

        Ok(())
    }

    fn write_accounts_to_file(
        &self,
        validator_type: ValidatorType,
        account_types: Vec<&str>,
        keypairs: Vec<Keypair>, //TODO: reference this
        deployment_tag: Option<String>,
    ) -> Result<(), Box<dyn Error>> {
        for (i, keypair) in keypairs.iter().enumerate() {
            let account_index = i / account_types.len();
            let account = account_types[i % account_types.len()];
            let mut filename_prefix = validator_type.to_string();
            if let Some(ref tag) = deployment_tag {
                filename_prefix = add_tag_to_name(filename_prefix.as_str(), tag);
            }
            let filename = match validator_type {
                ValidatorType::Bootstrap => {
                    format!("{}/{}.json", filename_prefix, account)
                }
                ValidatorType::Standard => format!(
                    "{}-{}-{}.json",
                    filename_prefix,
                    account,
                    account_index
                ),
                ValidatorType::RPC => format!(
                    "{}-{}-{}.json",
                    filename_prefix,
                    account,
                    account_index
                ),
                ValidatorType::Client => panic!("Client type not supported"),
            };

            let outfile = self.config_dir.join(&filename);
            write_keypair_file(&keypair, outfile)?;
        }
        Ok(())
    }

    fn setup_genesis_flags(&self) -> Vec<String> {
        let mut args = vec![
            "--bootstrap-validator-lamports".to_string(),
            sol_to_lamports(
                self.flags
                    .bootstrap_validator_sol
                    .unwrap_or(DEFAULT_BOOTSTRAP_NODE_SOL),
            )
            .to_string(),
            "--bootstrap-validator-stake-lamports".to_string(),
            sol_to_lamports(
                self.flags
                    .bootstrap_validator_stake_sol
                    .unwrap_or(DEFAULT_BOOTSTRAP_NODE_STAKE_SOL),
            )
            .to_string(),
            "--hashes-per-tick".to_string(),
            self.flags.hashes_per_tick.clone(),
            "--max-genesis-archive-unpacked-size".to_string(),
            self.flags
                .max_genesis_archive_unpacked_size
                .unwrap_or(DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE)
                .to_string(),
            "--faucet-lamports".to_string(),
            self.flags
                .faucet_lamports
                .unwrap_or(DEFAULT_FAUCET_LAMPORTS)
                .to_string(),
            "--faucet-pubkey".to_string(),
            self.config_dir
                .join("faucet.json")
                .to_string_lossy()
                .to_string(),
            "--cluster-type".to_string(),
            self.flags.cluster_type.to_string(),
            "--ledger".to_string(),
            self.config_dir
                .join("bootstrap-validator")
                .to_string_lossy()
                .to_string(),
        ];

        if self.flags.enable_warmup_epochs {
            args.push("--enable-warmup-epochs".to_string());
        }

        args.push("--bootstrap-validator".to_string());
        ["identity", "vote-account", "stake-account"]
            .iter()
            .for_each(|account_type| {
                args.push(
                    self.config_dir
                        .join(format!("bootstrap-validator/{}.json", account_type))
                        .to_string_lossy()
                        .to_string(),
                );
            });

        if let Some(slots_per_epoch) = self.flags.slots_per_epoch {
            args.push("--slots-per-epoch".to_string());
            args.push(slots_per_epoch.to_string());
        }

        if let Some(lamports_per_signature) = self.flags.target_lamports_per_signature {
            args.push("--target-lamports-per-signature".to_string());
            args.push(lamports_per_signature.to_string());
        }

        if self.config_dir.join("client-accounts.yml").exists() {
            args.push("--primordial-accounts-file".to_string());
            args.push(
                self.config_dir
                    .join("client-accounts.yml")
                    .to_string_lossy()
                    .to_string(),
            );
        }

        args
    }

    pub fn setup_spl_args(&self, solana_root_path: PathBuf) -> Result<Vec<String>, Box<dyn Error>> {
        let fetch_spl_file = solana_root_path.join("fetch-spl.sh");
        fetch_spl(&fetch_spl_file)?;

        // add in spl
        let spl_file = solana_root_path.join("spl-genesis-args.sh");
        parse_spl_genesis_file(&spl_file)
    }

    pub fn generate(
        &mut self,
        solana_root_path: PathBuf,
        build_path: PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        let mut args = self.setup_genesis_flags();
        let mut spl_args = self.setup_spl_args(solana_root_path)?;
        args.append(&mut spl_args);

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{SUN}Building Genesis..."));

        let executable_path = build_path.join("solana-genesis");
        let output = Command::new(executable_path)
            .args(&args)
            .output()
            .expect("Failed to execute solana-genesis");

        progress_bar.finish_and_clear();

        if !output.status.success() {
            return Err(format!(
                "Failed to create genesis. err: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        info!("Genesis build complete");

        Ok(())
    }

    pub fn create_client_accounts(
        &mut self,
        number_of_clients: usize,
        target_lamports_per_signature: u64,
        config_dir: &PathBuf,
        deploy_method: &DeployMethod,
        solana_root_path: &PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        let client_accounts_file = config_dir.join("client-accounts.yml");

        info!("generating {number_of_clients} client account(s)...");
        let _ = (0..number_of_clients).into_par_iter().try_for_each(|i| {
            info!("client account: {i}");
            let mut args = Vec::new();
            let account_path = config_dir.join(format!("bench-tps-{i}.yml"));
            args.push("--write-client-keys".to_string());
            args.push(account_path.to_string_lossy().to_string());
            args.push("--target-lamports-per-signature".to_string());
            args.push(target_lamports_per_signature.to_string());

            let executable_path = if let DeployMethod::ReleaseChannel(_) = deploy_method {
                solana_root_path.join("solana-release/bin/solana-bench-tps")
            } else {
                solana_root_path.join("farf/bin/solana-bench-tps")
            };

            Self::create_client_account(&args, &executable_path)
        });

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{WRITING}Writing client accounts..."));
        for i in 0..number_of_clients {
            let account_path = config_dir.join(format!("bench-tps-{i}.yml"));
            append_client_accounts_to_file(&account_path, &client_accounts_file)?;
        }
        progress_bar.finish_and_clear();
        info!("client-accounts.yml creation for genesis complete");

        Ok(())
    }

    fn create_client_account(
        args: &Vec<String>,
        executable_path: &PathBuf,
    ) -> Result<(), AnyHowError> {
        let output = Command::new(executable_path)
            .args(args)
            .output()
            .expect("Failed to execute solana-bench-tps");

        if !output.status.success() {
            return Err(AnyHowError::msg(format!(
                "Failed to create client accounts. err: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }
}
