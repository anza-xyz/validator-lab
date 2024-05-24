use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub num_clients: usize,
    pub client_type: String,
    pub client_to_run: String,
    pub bench_tps_args: Vec<String>,
    pub client_target_node: Option<Pubkey>,
    pub client_duration_seconds: u64,
    pub client_wait_for_n_nodes: Option<u64>,
    pub run_client: bool,
}
