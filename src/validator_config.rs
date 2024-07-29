use solana_sdk::pubkey::Pubkey;

#[derive(Debug)]
pub struct ValidatorConfig {
    pub internal_node_sol: f64,
    pub internal_node_stake_sol: f64,
    pub commission: u8,
    pub shred_version: Option<u16>,
    pub max_ledger_size: Option<u64>,
    pub skip_poh_verify: bool,
    pub no_snapshot_fetch: bool,
    pub require_tower: bool,
    pub enable_full_rpc: bool,
    pub known_validators: Vec<Pubkey>,
}
