use std::{env, path::PathBuf};

#[macro_export]
macro_rules! boxed_error {
    ($message:expr) => {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, $message)) as Box<dyn Error + Send>
    };
}

pub fn get_solana_root() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Failed to get Solana root directory")
        .to_path_buf()
}

pub struct SolanaRoot {
    root_path: PathBuf,
}

impl Default for SolanaRoot {
    fn default() -> Self {
        Self {
            root_path: get_solana_root(),
        }
    }
}

impl SolanaRoot {
    pub fn new_from_path(path: PathBuf) -> Self {
        Self { root_path: path }
    }

    pub fn get_root_path(&self) -> PathBuf {
        self.root_path.clone()
    }
}

pub mod kubernetes;
pub mod release;
