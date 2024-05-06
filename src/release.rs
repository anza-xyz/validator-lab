use {
    crate::{cat_file, download_to_temp, extract_release_archive},
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

#[derive(PartialEq, EnumString, IntoStaticStr, VariantNames)]
#[strum(serialize_all = "lowercase")]
pub enum BuildType {
    Skip, // use Agave build from the previous run
    Debug,
    Release,
}

pub struct BuildConfig {
    deploy_method: DeployMethod,
    build_type: BuildType,
    solana_root_path: PathBuf,
    docker_build: bool,
}

impl BuildConfig {
    pub fn new(
        deploy_method: DeployMethod,
        build_type: BuildType,
        solana_root_path: &Path,
        docker_build: bool,
    ) -> Self {
        BuildConfig {
            deploy_method,
            build_type,
            solana_root_path: solana_root_path.to_path_buf(),
            docker_build,
        }
    }

    pub fn docker_build(&self) -> bool {
        self.docker_build
    }

    pub async fn prepare(&self) -> Result<(), Box<dyn Error>> {
        if self.build_type == BuildType::Skip {
            info!("skipping build");
            return Ok(());
        }
        match &self.deploy_method {
            DeployMethod::ReleaseChannel(channel) => {
                let tar_directory = self.setup_tar_deploy(channel).await?;
                info!("Successfully setup tar file");
                cat_file(&tar_directory.join("version.yml"))?;
            }
            DeployMethod::Local(_) => {
                self.build()?;
            }
        }
        info!("Completed Prepare Deploy");
        Ok(())
    }

    async fn setup_tar_deploy(&self, release_channel: &String) -> Result<PathBuf, Box<dyn Error>> {
        let file_name = "solana-release";
        let tar_filename = format!("{file_name}.tar.bz2");
        info!("tar file: {tar_filename}");
        self.download_release_from_channel(&tar_filename, release_channel)
            .await?;

        // Extract it and load the release version metadata
        let tarball_filename = self.solana_root_path.join(&tar_filename);
        let release_dir = self.solana_root_path.join(file_name);
        extract_release_archive(&tarball_filename, &self.solana_root_path).map_err(|err| {
            format!(
                "Unable to extract {tar_filename} into {}: {err}",
                release_dir.display()
            )
        })?;

        Ok(release_dir)
    }

    fn build(&self) -> Result<(), Box<dyn Error>> {
        let start_time = Instant::now();
        let build_variant = if self.build_type == BuildType::Debug {
            "--debug"
        } else {
            ""
        };

        let install_directory = self.solana_root_path.join("farf");
        let install_script = self.solana_root_path.join("scripts/cargo-install-all.sh");
        match std::process::Command::new(install_script)
            .arg(install_directory)
            .arg(build_variant)
            .arg("--validator-only")
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

        let solana_repo = Repository::open(self.solana_root_path.as_path())?;
        let commit = solana_repo.revparse_single("HEAD")?.id();
        let branch = solana_repo
            .head()?
            .shorthand()
            .expect("Failed to get shortened branch name")
            .to_string();

        // Check if current commit is associated with a tag
        let mut note = branch;
        for tag in (&solana_repo.tag_names(None)?).into_iter().flatten() {
            // Get the target object of the tag
            let tag_object = solana_repo.revparse_single(tag)?.id();
            // Check if the commit associated with the tag is the same as the current commit
            if tag_object == commit {
                info!("The current commit is associated with tag: {tag}");
                note = tag_object.to_string();
                break;
            }
        }

        // Write to branch/tag and commit to version.yml
        let content = format!("channel: devbuild {note}\ncommit: {commit}");
        std::fs::write(self.solana_root_path.join("farf/version.yml"), content)
            .expect("Failed to write version.yml");

        info!("Build took {:.3?} seconds", start_time.elapsed());
        Ok(())
    }

    async fn download_release_from_channel(
        &self,
        tar_filename: &str,
        release_channel: &String,
    ) -> Result<(), Box<dyn Error>> {
        info!("Downloading release from channel: {release_channel}");
        let file_path = self.solana_root_path.join(tar_filename);
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
