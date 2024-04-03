use {
    log::*,
    rand::Rng,
    solana_core::gen_keys::GenKeys,
    solana_sdk::signature::{Keypair, write_keypair_file},
    std::{
        error::Error, 
        path::PathBuf,
        result::Result,
    },
};

fn output_keypair(keypair: &Keypair, outfile: &str) -> Result<(), Box<dyn Error>> {
    write_keypair_file(keypair, outfile)?;
    Ok(())
}
pub struct Genesis {
    pub config_dir: PathBuf,
}

impl Genesis {
    pub fn new(solana_root: PathBuf) -> Self {
        let config_dir = solana_root.join("config-k8s");
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir).unwrap();
        }
        std::fs::create_dir_all(&config_dir).unwrap();
        Genesis { config_dir }
    }

    pub fn generate_faucet(&self) -> Result<(), Box<dyn Error>> {
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

        if let Some(outfile) = outfile.to_str() {
            output_keypair(&keypair, outfile)
                .map_err(|err| format!("Unable to write {outfile}: {err}"))?;
        }
        Ok(())
    }
}
