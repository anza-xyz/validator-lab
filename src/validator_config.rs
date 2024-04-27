use solana_sdk::pubkey::Pubkey;

#[derive(Debug)]
pub struct ValidatorConfig {
    pub max_ledger_size: Option<u64>,
    pub skip_poh_verify: bool,
    pub no_snapshot_fetch: bool,
    pub require_tower: bool,
    pub enable_full_rpc: bool,
    pub known_validators: Vec<Pubkey>,
}
