use std::path::PathBuf;

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
}
