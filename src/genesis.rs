use {
    crate::ValidatorType,
    log::*,
    rand::Rng,
    solana_core::gen_keys::GenKeys,
    solana_sdk::signature::{write_keypair_file, Keypair},
    std::{error::Error, path::PathBuf, result::Result},
};

pub struct Genesis {
    config_dir: PathBuf,
    key_generator: GenKeys,
}

impl Genesis {
    pub fn new(solana_root: PathBuf) -> Self {
        let config_dir = solana_root.join("config-k8s");
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).unwrap();
        }
        std::fs::create_dir_all(&config_dir).unwrap();

        let seed: [u8; 32] = rand::thread_rng().gen();

        Self {
            config_dir,
            key_generator: GenKeys::new(seed),
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
    ) -> Result<(), Box<dyn Error>> {
        if validator_type == ValidatorType::Client {
            return Err("Client valdiator_type in generate_accounts not allowed".into());
        }

        info!(
            "generating {} {} accounts...",
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

        self.write_accounts_to_file(validator_type, account_types, keypairs)?;

        Ok(())
    }

    fn write_accounts_to_file(
        &self,
        validator_type: ValidatorType,
        account_types: Vec<&str>,
        keypairs: Vec<Keypair>, //TODO: reference this
    ) -> Result<(), Box<dyn Error>> {
        for (i, keypair) in keypairs.iter().enumerate() {
            let account_index = i / account_types.len();
            let account = account_types[i % account_types.len()];
            let filename = match validator_type {
                ValidatorType::Bootstrap => {
                    format!("{}/{}.json", validator_type, account)
                }
                ValidatorType::Standard => {
                    format!("{}-{}-{}.json", validator_type, account, account_index)
                }
                ValidatorType::RPC => {
                    format!("{}-{}-{}.json", validator_type, account, account_index)
                }
                ValidatorType::Client => panic!("Client type not supported"),
            };

            let outfile = self.config_dir.join(&filename);
            write_keypair_file(&keypair, outfile)?;
        }
        Ok(())
    }
}
