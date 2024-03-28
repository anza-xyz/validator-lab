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
        let seed: [u8; 32] = rand::thread_rng().gen();
        let keypair = GenKeys::new(seed).gen_keypair();

        if let Some(outfile) = outfile.to_str() {
            output_keypair(&keypair, outfile)
                .map_err(|err| format!("Unable to write {outfile}: {err}"))?;
        }
        Ok(())
    }
}
