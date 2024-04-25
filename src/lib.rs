use {
    bzip2::bufread::BzDecoder,
    console::Emoji,
    indicatif::{ProgressBar, ProgressStyle},
    log::*,
    reqwest::Client,
    std::{
        env,
        error::Error,
        fmt::{self, Display, Formatter},
        fs::File,
        io::{BufReader, Cursor, Read},
        path::{Path, PathBuf},
        time::Duration,
    },
    tar::Archive,
    url::Url,
};

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

    pub fn get_root_path(&self) -> PathBuf {
        self.root_path.clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidatorType {
    Bootstrap,
    Standard,
    RPC,
    Client,
}

impl std::fmt::Display for ValidatorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            ValidatorType::Bootstrap => write!(f, "bootstrap-validator"),
            ValidatorType::Standard => write!(f, "validator"),
            ValidatorType::RPC => write!(f, "rpc-node"),
            ValidatorType::Client => write!(f, "client"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics {
    pub host: String,
    pub port: String,
    pub database: String,
    pub username: String,
    password: String,
}

impl Metrics {
    pub fn new(
        host: String,
        port: String,
        database: String,
        username: String,
        password: String,
    ) -> Self {
        Metrics {
            host,
            port,
            database,
            username,
            password,
        }
    }
    pub fn to_env_string(&self) -> String {
        format!(
            "host={}:{},db={},u={},p={}",
            self.host, self.port, self.database, self.username, self.password
        )
    }
}

#[derive(Debug)]
struct DockerPushThreadError {
    message: String,
}

impl Display for DockerPushThreadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for DockerPushThreadError {}

impl From<String> for DockerPushThreadError {
    fn from(message: String) -> Self {
        DockerPushThreadError { message }
    }
}

impl From<&str> for DockerPushThreadError {
    fn from(message: &str) -> Self {
        DockerPushThreadError {
            message: message.to_string(),
        }
    }
}

pub mod client_config;
pub mod docker;
pub mod genesis;
pub mod k8s_helpers;
pub mod kubernetes;
pub mod ledger_helper;
pub mod library;
pub mod release;
pub mod validator;
pub mod validator_config;

static BUILD: Emoji = Emoji("👷 ", "");
static PACKAGE: Emoji = Emoji("📦 ", "");
static ROCKET: Emoji = Emoji("🚀 ", "");
static SUN: Emoji = Emoji("🌞 ", "");
static TRUCK: Emoji = Emoji("🚚 ", "");
static WRITING: Emoji = Emoji("🖊️ ", "");

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
    info!("{:?}:\n{}", path.file_name(), contents);

    Ok(())
}

pub async fn download_to_temp(
    url: &str,
    file_name: &str,
    solana_root_path: PathBuf,
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

    let file_name: PathBuf = solana_root_path.join(file_name);
    let mut out = File::create(file_name).expect("failed to create file");
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

pub fn add_tag_to_name(name: &str, tag: &str) -> String {
    let mut name_with_tag = name.to_string();
    name_with_tag.push('-');
    name_with_tag.push_str(tag);
    name_with_tag
}
