use {
    crate::{genesis::DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE, LEDGER_DIR},
    log::*,
    solana_accounts_db::hardened_unpack::open_genesis_config,
    solana_sdk::shred_version::compute_shred_version,
    std::error::Error,
};

fn ledger_directory_exists() -> Result<(), Box<dyn Error>> {
    if !LEDGER_DIR.exists() {
        return Err(format!(
            "Ledger Directory does not exist, have you created genesis yet??"
        ).into());
    }
    Ok(())
}

pub struct LedgerHelper {}

impl LedgerHelper {
    pub fn get_shred_version() -> Result<u16, Box<dyn Error>> {
        ledger_directory_exists()?;
        let genesis_config = open_genesis_config(
            LEDGER_DIR.as_path(),
            DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
        );
        let shred_version = compute_shred_version(&genesis_config?.hash(), None);
        info!("Shred Version: {}", shred_version);
        Ok(shred_version)
    }
}