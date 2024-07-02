use {
    solana_sdk::pubkey::Pubkey,
    std::{error::Error, path::PathBuf},
    strum_macros::Display,
};

#[derive(Clone, PartialEq, Debug)]
pub struct BenchTpsConfig {
    pub num_clients: usize,
    pub client_duration_seconds: u64,
    pub client_type: String,
    pub bench_tps_args: Vec<String>,
    pub client_wait_for_n_nodes: Option<usize>,
    pub client_to_run: String,
    pub client_target_node: Option<Pubkey>,
}

impl ClientTrait for BenchTpsConfig {
    fn generate_client_command_flags(&self) -> Vec<String> {
        let mut flags = vec![];

        flags.push(self.client_to_run.clone()); //client to run
        if !self.bench_tps_args.is_empty() {
            flags.push(self.bench_tps_args.join(" "));
        }

        flags.push(self.client_type.clone());

        if let Some(target_node) = self.client_target_node {
            flags.push("--target-node".to_string());
            flags.push(target_node.to_string());
        }

        flags.push("--duration".to_string());
        flags.push(self.client_duration_seconds.to_string());

        if let Some(num_nodes) = self.client_wait_for_n_nodes {
            flags.push("--num-nodes".to_string());
            flags.push(num_nodes.to_string());
        }

        flags
    }

    fn build_command(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let mut command =
            vec!["/home/solana/k8s-cluster-scripts/client-startup-script.sh".to_string()];
        command.extend(self.generate_client_command_flags());
        Ok(command)
    }
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct GenericClientConfig {
    pub num_clients: usize,
    pub client_duration_seconds: u64,
    pub args: Vec<String>,
    pub image: String,
    pub executable_path: PathBuf,
    pub delay_start: u64,
}

impl ClientTrait for GenericClientConfig {
    fn generate_client_command_flags(&self) -> Vec<String> {
        self.args.clone()
    }

    /// Build command to run on pod deployment
    fn build_command(&self) -> Result<Vec<String>, Box<dyn Error>> {
        let exec_path_string = self
            .executable_path
            .clone()
            .into_os_string()
            .into_string()
            .map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid Unicode data in path: {:?}", err),
                )
            })?;
        let mut command = vec![exec_path_string];
        command.extend(self.generate_client_command_flags());
        Ok(command)
    }
}

#[derive(Debug, Clone, PartialEq, Display)]
pub enum ClientConfig {
    #[strum(serialize = "bench-tps")]
    BenchTps(BenchTpsConfig),
    #[strum(serialize = "generic")]
    Generic(GenericClientConfig),
}

impl ClientConfig {
    pub fn num_clients(&self) -> usize {
        match self {
            ClientConfig::BenchTps(config) => config.num_clients,
            ClientConfig::Generic(config) => config.num_clients,
        }
    }

    pub fn build_command(&self) -> Result<Vec<String>, Box<dyn Error>> {
        match self {
            ClientConfig::BenchTps(config) => config.build_command(),
            ClientConfig::Generic(config) => config.build_command(),
        }
    }
}

pub trait ClientTrait {
    fn generate_client_command_flags(&self) -> Vec<String>; // Add this method
    fn build_command(&self) -> Result<Vec<String>, Box<dyn Error>>;
}
