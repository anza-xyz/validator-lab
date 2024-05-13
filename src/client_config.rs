use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub num_clients: usize,
    pub client_type: String,
    pub client_to_run: String,
    pub bench_tps_args: Vec<String>,
    pub target_node: Option<Pubkey>,
    pub duration: u64,
    pub num_nodes: Option<u64>,
}
