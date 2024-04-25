use solana_sdk::pubkey::Pubkey;

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub num_clients: usize,
    pub client_delay_start: u64,
    pub client_type: String,
    pub client_to_run: String,
    pub bench_tps_args: Option<Vec<String>>,
    pub target_node: Option<Pubkey>,
    pub duration: u64,
    pub num_nodes: Option<u64>,
    pub run_client: bool,
}

pub fn parse_and_format_bench_tps_args(bench_tps_args: Option<&str>) -> Option<Vec<String>> {
    bench_tps_args.map(|args| {
        let mut val_args: Vec<_> = args
            .split_whitespace()
            .filter_map(|arg| arg.split_once('='))
            .flat_map(|(key, value)| vec![format!("--{}", key), value.to_string()])
            .collect();
        let flag_args_iter = args
            .split_whitespace()
            .filter_map(|arg| match arg.split_once('=') {
                Some(_) => None,
                None => Some(arg),
            })
            .map(|flag| format!("--{}", flag));
        val_args.extend(flag_args_iter);
        val_args
    })
}
