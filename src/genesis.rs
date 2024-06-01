use {
    crate::{fetch_spl, new_spinner_progress_bar, ValidatorType, SOLANA_RELEASE, SUN, WRITING},
    log::*,
    rand::Rng,
    solana_core::gen_keys::GenKeys,
    solana_sdk::{
        native_token::sol_to_lamports,
        signature::{write_keypair_file, Keypair},
    },
    std::{
        error::Error,
        fs::{File, OpenOptions},
        io::{self, BufRead, BufWriter, Read, Write},
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
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

fn parse_spl_genesis_file(
    spl_file: &PathBuf,
    solana_root_path: &Path,
) -> Result<Vec<String>, Box<dyn Error>> {
    // Read entire file into a String
    let mut file = File::open(spl_file)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let args = content
        .split_whitespace()
        .map(String::from)
        .map(|arg| {
            if arg.ends_with(".so") {
                solana_root_path
                    .join(&arg)
                    .into_os_string()
                    .into_string()
                    .unwrap()
            } else {
                arg
            }
        })
        .collect::<Vec<String>>();

    Ok(args)
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

fn append_client_accounts_to_file(
    bench_tps_account_path: &PathBuf, //bench-tps-i.yml
    client_accounts_path: &PathBuf,   //client-accounts.yml
) -> io::Result<()> {
    // Open the bench-tps-i.yml file for reading.
    let input = File::open(bench_tps_account_path)?;
    let reader = io::BufReader::new(input);

    // Open (or create) client-accounts.yml
    let output = OpenOptions::new()
        .create(true)
        .append(true)
        .open(client_accounts_path)?;
    let mut writer = BufWriter::new(output);

    // Skip first line since it is a header aka "---" in a yaml
    for line in reader.lines().skip(1) {
        let line = line?;
        writeln!(writer, "{line}")?;
    }

    Ok(())
}

pub struct Genesis {
    config_dir: PathBuf,
    key_generator: GenKeys,
    pub flags: GenesisFlags,
}

impl Genesis {
    pub fn new(config_dir: PathBuf, flags: GenesisFlags, retain_previous_genesis: bool) -> Self {
        // if we are deploying a heterogeneous cluster
        // all deployments after the first must retain the original genesis directory
        if !retain_previous_genesis {
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
        deployment_tag: Option<&str>,
    ) -> Result<(), Box<dyn Error>> {
        info!("generating {number_of_accounts} {validator_type} accounts...");

        let account_types = match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard => {
                vec!["identity", "stake-account", "vote-account"]
            }
            ValidatorType::RPC => {
                vec!["identity"] // no vote or stake account for RPC
            }
            ValidatorType::Client(_) => {
                return Err("Client valdiator_type in generate_accounts not allowed".into())
            }
        };

        let account_types: Vec<String> = if let Some(tag) = deployment_tag {
            account_types
                .into_iter()
                .map(|acct| format!("{}-{}", acct, tag))
                .collect()
        } else {
            account_types
                .into_iter()
                .map(|acct| acct.to_string())
                .collect()
        };

        let total_accounts_to_generate = number_of_accounts * account_types.len();
        let keypairs = self
            .key_generator
            .gen_n_keypairs(total_accounts_to_generate as u64);

        self.write_accounts_to_file(&validator_type, &account_types, &keypairs)?;

        Ok(())
    }

    fn write_accounts_to_file(
        &self,
        validator_type: &ValidatorType,
        account_types: &[String],
        keypairs: &[Keypair],
    ) -> Result<(), Box<dyn Error>> {
        for (i, keypair) in keypairs.iter().enumerate() {
            let account_index = i / account_types.len();
            let account = &account_types[i % account_types.len()];
            let filename = match validator_type {
                ValidatorType::Bootstrap => {
                    format!("{validator_type}/{account}.json")
                }
                ValidatorType::Standard | ValidatorType::RPC => {
                    format!("{validator_type}-{account}-{account_index}.json")
                }
                ValidatorType::Client(_) => panic!("Client type not supported"),
            };

            let outfile = self.config_dir.join(&filename);
            write_keypair_file(keypair, outfile)?;
        }
        Ok(())
    }

    pub fn create_client_accounts(
        &mut self,
        number_of_clients: usize,
        bench_tps_args: &[String],
        target_lamports_per_signature: u64,
        config_dir: &Path,
        solana_root_path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        if number_of_clients == 0 {
            return Ok(());
        }

        let client_accounts_file = config_dir.join("client-accounts.yml");

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{WRITING}Creating and writing client accounts..."));

        info!("generating {number_of_clients} client account(s)...");
        let children: Result<Vec<_>, _> = (0..number_of_clients)
            .map(|i| {
                Self::create_client_account(
                    i,
                    config_dir,
                    target_lamports_per_signature,
                    bench_tps_args,
                    solana_root_path,
                )
            })
            .collect();

        for child in children? {
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(output.status.to_string().into());
            }
        }

        for i in 0..number_of_clients {
            let account_path = config_dir.join(format!("bench-tps-{i}.yml"));
            append_client_accounts_to_file(&account_path, &client_accounts_file)?;
        }
        progress_bar.finish_and_clear();
        info!("client-accounts.yml creation for genesis complete");

        Ok(())
    }

    fn create_client_account(
        client_index: usize,
        config_dir: &Path,
        target_lamports_per_signature: u64,
        bench_tps_args: &[String],
        solana_root_path: &Path,
    ) -> Result<Child, Box<dyn Error>> {
        info!("client account: {client_index}");
        let mut args = Vec::new();
        let account_path = config_dir.join(format!("bench-tps-{client_index}.yml"));
        debug!("account path: {account_path:?}");
        args.push("--write-client-keys".to_string());
        args.push(account_path.into_os_string().into_string().map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid Unicode data in path: {:?}", err),
            )
        })?);
        args.push("--target-lamports-per-signature".to_string());
        args.push(target_lamports_per_signature.to_string());

        args.extend_from_slice(bench_tps_args);

        let executable_path =
            solana_root_path.join(format!("{SOLANA_RELEASE}/bin/solana-bench-tps"));

        let child = Command::new(executable_path)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(child)
    }

    fn setup_genesis_flags(&self) -> Result<Vec<String>, Box<dyn Error>> {
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
                .into_os_string()
                .into_string()
                .map_err(|err| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid Unicode data in path: {:?}", err),
                    )
                })?,
            "--cluster-type".to_string(),
            self.flags.cluster_type.to_string(),
            "--ledger".to_string(),
            self.config_dir
                .join("bootstrap-validator")
                .into_os_string()
                .into_string()
                .map_err(|err| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Invalid Unicode data in path: {:?}", err),
                    )
                })?,
        ];

        if self.flags.enable_warmup_epochs {
            args.push("--enable-warmup-epochs".to_string());
        }

        args.push("--bootstrap-validator".to_string());
        for account_type in ["identity", "vote-account", "stake-account"].iter() {
            let path = self
                .config_dir
                .join(format!("bootstrap-validator/{account_type}.json"))
                .into_os_string()
                .into_string()
                .map_err(|_| "Failed to convert path to string")?;
            args.push(path);
        }

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
                    .into_os_string()
                    .into_string()
                    .map_err(|err| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Invalid Unicode data in path: {:?}", err),
                        )
                    })?,
            );
        }

        Ok(args)
    }

    pub async fn setup_spl_args(
        &self,
        solana_root_path: &Path,
    ) -> Result<Vec<String>, Box<dyn Error>> {
        fetch_spl(solana_root_path).await?;

        let spl_file = solana_root_path.join("spl-genesis-args.sh");
        parse_spl_genesis_file(&spl_file, solana_root_path)
    }

    pub async fn generate(
        &mut self,
        solana_root_path: &Path,
        exec_path: &Path,
    ) -> Result<(), Box<dyn Error>> {
        let mut args = self.setup_genesis_flags()?;
        let mut spl_args = self.setup_spl_args(solana_root_path).await?;
        args.append(&mut spl_args);

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{SUN}Building Genesis..."));

        let executable_path = exec_path.join("solana-genesis");
        let output = Command::new(executable_path)
            .args(&args)
            .output()
            .expect("Failed to execute solana-genesis");

        progress_bar.finish_and_clear();

        if !output.status.success() {
            return Err(format!(
                "Failed to create genesis. err: {:?}",
                String::from_utf8(output.stderr)
            )
            .into());
        }
        info!("Genesis build complete");

        Ok(())
    }
}
