use {
    crate::genesis::DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE,
    solana_accounts_db::hardened_unpack::open_genesis_config,
    solana_sdk::shred_version::compute_shred_version,
    std::{error::Error, path::Path},
};

fn ledger_directory_exists(ledger_dir: &Path) -> Result<(), Box<dyn Error>> {
    if !ledger_dir.exists() {
        return Err(
            "Ledger Directory does not exist, have you created genesis yet??"
                .to_string()
                .into(),
        );
    }
    Ok(())
}

pub struct LedgerHelper {}

impl LedgerHelper {
    pub fn get_shred_version(ledger_dir: &Path) -> Result<u16, Box<dyn Error>> {
        ledger_directory_exists(ledger_dir)?;
        let genesis_config =
            open_genesis_config(ledger_dir, DEFAULT_MAX_GENESIS_ARCHIVE_UNPACKED_SIZE);
        let shred_version = compute_shred_version(&genesis_config?.hash(), None);
        Ok(shred_version)
    }
}
