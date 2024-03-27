use {
    bzip2::bufread::BzDecoder,
    console::Emoji,
    indicatif::{ProgressBar, ProgressStyle},
    log::*,
    reqwest::Client,
    std::{
        env,
        error::Error,
        fs::{self, File},
        io::{BufReader, Cursor, Read},
        path::{Path, PathBuf},
        time::Duration,
    },
    tar::Archive,
    url::Url,
};

#[macro_export]
macro_rules! boxed_error {
    ($message:expr) => {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, $message)) as Box<dyn Error + Send>
    };
}

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

pub mod kubernetes;
pub mod release;

static PACKAGE: Emoji = Emoji("📦 ", "");
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
    info!("version.yml:\n{}", contents);

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
        return Err(boxed_error!(format!(
            "Failed to download release from url: {:?}, response body: {:?}",
            url.to_string(),
            response.text().await?
        )));
    }

    let file_name: PathBuf = solana_root_path.join(file_name);
    let mut out = File::create(file_name).expect("failed to create file");
    let mut content = Cursor::new(response.bytes().await?);
    std::io::copy(&mut content, &mut out)?;

    progress_bar.finish_and_clear();
    Ok(())
}

pub fn extract_release_archive(
    archive: &Path,
    extract_dir: &Path,
    file_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let progress_bar = new_spinner_progress_bar();
    progress_bar.set_message(format!("{PACKAGE}Extracting..."));

    if extract_dir.exists() {
        fs::remove_dir_all(extract_dir)?;
    }
    fs::create_dir_all(extract_dir)?;

    let tmp_extract_dir = extract_dir.with_file_name("tmp-extract");

    if tmp_extract_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_extract_dir);
    }
    fs::create_dir_all(&tmp_extract_dir)?;

    let tar_bz2 = File::open(archive)?;
    let tar = BzDecoder::new(BufReader::new(tar_bz2));
    let mut release = Archive::new(tar);
    release.unpack(&tmp_extract_dir)?;

    for entry in tmp_extract_dir.join(file_name).read_dir()? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_entry_path = extract_dir.join(entry_path.file_name().unwrap());
        fs::rename(entry_path, target_entry_path)?;
    }

    // Remove the tmp-extract directory
    fs::remove_dir_all(tmp_extract_dir)?;
    progress_bar.finish_and_clear();
    Ok(())
}
