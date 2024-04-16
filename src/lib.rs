use {
    bzip2::bufread::BzDecoder,
    console::Emoji,
    indicatif::{ProgressBar, ProgressStyle},
    log::*,
    reqwest::Client,
    std::{
        env,
        fs::File,
        io::{BufReader, Cursor, Read, Write},
        path::{Path, PathBuf},
        time::Duration,
    },
    strum_macros::Display,
    tar::Archive,
    url::Url,
};

const UPGRADEABLE_LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";

pub fn get_solana_root() -> PathBuf {
    PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR")).to_path_buf()
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

    pub fn get_root_path(&self) -> &PathBuf {
        &self.root_path
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Display)]
pub enum ValidatorType {
    #[strum(serialize = "bootstrap-validator")]
    Bootstrap,
    #[strum(serialize = "validator")]
    Standard,
    #[strum(serialize = "rpc-node")]
    RPC,
    #[strum(serialize = "client")]
    Client,
}

pub mod docker;
pub mod genesis;
pub mod kubernetes;
pub mod release;

static BUILD: Emoji = Emoji("👷 ", "");
static PACKAGE: Emoji = Emoji("📦 ", "");
static ROCKET: Emoji = Emoji("🚀 ", "");
static SUN: Emoji = Emoji("🌞 ", "");
static TRUCK: Emoji = Emoji("🚚 ", "");

/// Creates a new process bar for processing that will take an unknown amount of time
pub fn new_spinner_progress_bar() -> ProgressBar {
    let progress_bar = ProgressBar::new(42);
    progress_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {wide_msg}")
            .expect("ProgresStyle::template direct input to be correct"),
    );
    progress_bar.enable_steady_tick(Duration::from_millis(100));
    progress_bar
}

pub fn cat_file(path: &PathBuf) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    info!("{:?}:\n{contents}", path.file_name());

    Ok(())
}

pub async fn download_to_temp(
    url: &str,
    file_path: &Path, // full path to file including filename
) -> Result<(), Box<dyn std::error::Error>> {
    let progress_bar = new_spinner_progress_bar();
    progress_bar.set_message(format!("{TRUCK}Downloading..."));

    let url = Url::parse(url).map_err(|err| format!("Unable to parse {url}: {err}"))?;

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .https_only(false)
        .build()?;

    let response = client.get(url.clone()).send().await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download release from url: {:?}, response body: {:?}",
            url.to_string(),
            response.text().await?
        )
        .into());
    }

    let mut out = File::create(file_path).expect("failed to create file");
    let mut content = Cursor::new(response.bytes().await?);
    std::io::copy(&mut content, &mut out)?;

    progress_bar.finish_and_clear();
    Ok(())
}

pub fn extract_release_archive(
    tarball_filename: &Path,
    extract_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let progress_bar = new_spinner_progress_bar();
    progress_bar.set_message(format!("{PACKAGE}Extracting..."));

    let tarball_file = File::open(tarball_filename)?;
    let decompressed = BzDecoder::new(BufReader::new(tarball_file));
    let mut archive = Archive::new(decompressed);

    // Unpack the archive into extract_dir
    archive.unpack(extract_dir)?;

    progress_bar.finish_and_clear();

    Ok(())
}

async fn fetch_program(
    name: &str,
    version: &str,
    solana_root_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let so_filename = format!("spl_{}-{}.so", name.replace('-', "_"), version);
    let so_path = solana_root_path.join(&so_filename);

    if !so_path.exists() {
        info!("Downloading {} {}", name, version);
        let url = format!(
            "https://github.com/solana-labs/solana-program-library/releases/download/{}-v{}/{}",
            name, version, so_filename
        );

        download_to_temp(&url, &so_path)
            .await
            .map_err(|err| format!("Unable to download {url}. Error: {err}"))?;
    }

    Ok(())
}

pub async fn fetch_spl(solana_root_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut genesis_args = vec![];

    let programs = vec![
        (
            "token",
            "3.5.0",
            "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
            "BPFLoader2111111111111111111111111111111111",
        ),
        (
            "token-2022",
            "0.9.0",
            "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
            UPGRADEABLE_LOADER,
        ),
        (
            "associated-token-account",
            "1.1.2",
            "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
            "BPFLoader2111111111111111111111111111111111",
        ),
    ];

    for (name, version, address, loader) in programs {
        fetch_program(name, version, solana_root_path).await?;

        let arg = if loader == UPGRADEABLE_LOADER {
            format!(
                "--upgradeable-program {} {} spl_{}-{}.so none",
                address,
                loader,
                name.replace('-', "_"),
                version
            )
        } else {
            format!(
                "--bpf-program {} {} spl_{}-{}.so",
                address,
                loader,
                name.replace('-', "_"),
                version
            )
        };
        genesis_args.push(arg);
    }

    // Write genesis args to file
    let mut file = std::fs::File::create(solana_root_path.join("spl-genesis-args.sh"))?;
    writeln!(file, "{}", genesis_args.join(" "))?;

    Ok(())
}
