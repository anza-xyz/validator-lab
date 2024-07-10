use {
    crate::{cat_file, download_to_temp, extract_release_archive, SOLANA_RELEASE},
    git2::Repository,
    log::*,
    std::{
        error::Error,
        fs,
        path::{Path, PathBuf},
        time::Instant,
    },
    strum_macros::{EnumString, IntoStaticStr, VariantNames},
};

#[derive(Debug, PartialEq, Clone)]
pub enum DeployMethod {
    Local(String),
    ReleaseChannel(String),
}

#[derive(PartialEq, EnumString, IntoStaticStr, VariantNames, Clone)]
#[strum(serialize_all = "lowercase")]
pub enum BuildType {
    /// use Agave build from the previous run
    Skip,
    Debug,
    Release,
}

pub struct BuildConfig {
    deploy_method: DeployMethod,
    build_type: BuildType,
    cluster_root_path: PathBuf,
    agave_repo_path: Option<PathBuf>,
    /// solana-release directory holding all solana/agave bins
    install_directory: PathBuf,
}

impl BuildConfig {
    pub fn new(
        deploy_method: DeployMethod,
        build_type: BuildType,
        cluster_root_path: &Path,
        agave_repo_path: Option<PathBuf>,
    ) -> Self {
        // If the solana-release directory exists and we're not skipping the build, delete it and create a new one.
        let install_directory = cluster_root_path.join(SOLANA_RELEASE);
        if build_type != BuildType::Skip && install_directory.exists() {
            std::fs::remove_dir_all(&install_directory).unwrap();
        }
        std::fs::create_dir_all(&install_directory).unwrap();
        BuildConfig {
            deploy_method,
            build_type,
            cluster_root_path: cluster_root_path.to_path_buf(),
            agave_repo_path,
            install_directory,
        }
    }

    pub async fn prepare(&self) -> Result<String, Box<dyn Error>> {
        match &self.deploy_method {
            DeployMethod::ReleaseChannel(channel) => {
                if self.build_type == BuildType::Skip {
                    return Ok(channel.clone());
                }
                self.setup_tar_deploy(channel).await?;
                info!("Successfully setup tar file");
                cat_file(&self.install_directory.join("version.yml"))?;
                Ok(channel.clone())
            }
            DeployMethod::Local(_) => {
                let image_tag = self.build()?;
                Ok(image_tag)
            }
        }
    }

    async fn setup_tar_deploy(&self, release_channel: &String) -> Result<(), Box<dyn Error>> {
        let tar_filename = format!("{SOLANA_RELEASE}.tar.bz2");
        self.download_release_from_channel(&tar_filename, release_channel)
            .await?;

        // Extract it and load the release version metadata
        let tarball_filename = self.cluster_root_path.join(&tar_filename);
        extract_release_archive(&tarball_filename, &self.cluster_root_path).map_err(|err| {
            format!(
                "Unable to extract {tar_filename} into {}: {err}",
                self.install_directory.display()
            )
        })?;
        Ok(())
    }

    fn build(&self) -> Result<String, Box<dyn Error>> {
        let agave_path = match &self.agave_repo_path {
            Some(path) => path.clone(),
            None => return Err("An agave repo path must be configured to build, please specify `--cluster-data-path`".into()),
        };

        let start_time = Instant::now();
        if self.build_type != BuildType::Skip {
            let build_variant = if self.build_type == BuildType::Debug {
                "--debug"
            } else {
                ""
            };

            let install_script = agave_path.join("scripts/cargo-install-all.sh");
            match std::process::Command::new(install_script)
                .arg("--validator-only")
                .arg(self.install_directory.clone())
                .arg(build_variant)
                .status()
            {
                Ok(result) => {
                    if result.success() {
                        info!("Successfully built validator")
                    } else {
                        return Err("Failed to build validator".into());
                    }
                }
                Err(err) => return Err(Box::new(err)),
            }
        }

        let solana_repo = Repository::open(agave_path.as_path())?;
        let commit = solana_repo.revparse_single("HEAD")?.id();
        let branch = solana_repo
            .head()?
            .shorthand()
            .expect("Failed to get shortened branch name")
            .to_string();

        // Check if current commit is associated with a tag
        let mut note = branch;
        let mut commit_tag = None;
        for tag in (&solana_repo.tag_names(None)?).into_iter().flatten() {
            // Get the target object of the tag
            let tag_object = solana_repo.revparse_single(tag)?.id();
            // Check if the commit associated with the tag is the same as the current commit
            if tag_object == commit {
                info!("The current commit is associated with tag: {tag}");
                commit_tag = Some(tag.to_string());
                note = tag_object.to_string();
                break;
            }
        }

        // Write to branch/tag and commit to version.yml
        let content = format!("channel: devbuild {note}\ncommit: {commit}");
        std::fs::write(
            self.cluster_root_path
                .join(format!("{SOLANA_RELEASE}/version.yml")),
            content,
        )
        .expect("Failed to write version.yml");

        let label = commit_tag.unwrap_or_else(|| commit.to_string()[..8].to_string());

        info!("Build took {:.3?} seconds", start_time.elapsed());
        Ok(label)
    }

    async fn download_release_from_channel(
        &self,
        tar_filename: &str,
        release_channel: &String,
    ) -> Result<(), Box<dyn Error>> {
        info!("Downloading release from channel: {release_channel}");
        let file_path = self.cluster_root_path.join(tar_filename);
        // Remove file
        if let Err(err) = fs::remove_file(&file_path) {
            if err.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("{err}: {:?}", "Error while removing file:").into());
            }
        }

        let download_url = format!(
            "https://release.solana.com/{release_channel}/solana-release-x86_64-unknown-linux-gnu.tar.bz2"
        );
        info!("download_url: {download_url}");

        download_to_temp(download_url.as_str(), &file_path)
            .await
            .map_err(|err| format!("Unable to download {download_url}. Error: {err}"))?;

        Ok(())
    }
}
