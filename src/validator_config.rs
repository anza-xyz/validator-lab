use solana_sdk::pubkey::Pubkey;

pub struct ValidatorConfig {
    pub tpu_enable_udp: bool,
    pub tpu_disable_quic: bool,
    pub max_ledger_size: Option<u64>,
    pub skip_poh_verify: bool,
    pub no_snapshot_fetch: bool,
    pub require_tower: bool,
    pub enable_full_rpc: bool,
    pub known_validators: Option<Vec<Pubkey>>,
}

impl std::fmt::Display for ValidatorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let known_validators = match &self.known_validators {
            Some(validators) => validators
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            None => "None".to_string(),
        };
        write!(
            f,
            "Runtime Config\n\
             tpu_enable_udp: {}\n\
             tpu_disable_quic: {}\n\
             max_ledger_size: {:?}\n\
             skip_poh_verify: {}\n\
             no_snapshot_fetch: {}\n\
             require_tower: {}\n\
             enable_full_rpc: {}\n\
             known_validators: {:?}",
            self.tpu_enable_udp,
            self.tpu_disable_quic,
            self.max_ledger_size,
            self.skip_poh_verify,
            self.no_snapshot_fetch,
            self.require_tower,
            self.enable_full_rpc,
            known_validators,
        )
    }
}
