use {
    crate::{
        new_spinner_progress_bar, release::DeployMethod, startup_scripts::StartupScripts,
        validator::Validator, ValidatorType, BUILD, ROCKET,
    },
    log::*,
    std::{
        error::Error,
        fmt::{self, Display, Formatter},
        fs,
        path::{Path, PathBuf},
        process::{Child, Command, Stdio},
    },
};

#[derive(Clone)]
pub struct DockerImage {
    registry: String,
    validator_type: ValidatorType,
    image_name: String,
    tag: String,
}

impl DockerImage {
    // Constructor to create a new instance of DockerImage
    pub fn new(
        registry: String,
        validator_type: ValidatorType,
        image_name: String,
        tag: String,
    ) -> Self {
        DockerImage {
            registry,
            validator_type,
            image_name,
            tag,
        }
    }

    pub fn validator_type(&self) -> ValidatorType {
        self.validator_type
    }
}

// Put DockerImage in format for building, pushing, and pulling
impl Display for DockerImage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.validator_type {
            ValidatorType::Client(index) => write!(
                f,
                "{}/{}-{}-{}:{}",
                self.registry, self.validator_type, index, self.image_name, self.tag
            ),
            ValidatorType::Bootstrap | ValidatorType::Standard | ValidatorType::RPC => write!(
                f,
                "{}/{}-{}:{}",
                self.registry, self.validator_type, self.image_name, self.tag
            ),
        }
    }
}

pub struct DockerConfig {
    pub base_image: String,
    deploy_method: DeployMethod,
}

impl DockerConfig {
    pub fn new(base_image: String, deploy_method: DeployMethod) -> Self {
        DockerConfig {
            base_image,
            deploy_method,
        }
    }

    pub fn build_image(
        &self,
        solana_root_path: &Path,
        docker_image: &DockerImage,
    ) -> Result<(), Box<dyn Error>> {
        let validator_type = docker_image.validator_type();
        let docker_path = match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard | ValidatorType::RPC => {
                solana_root_path.join(format!("docker-build/{validator_type}"))
            }
            ValidatorType::Client(index) => {
                solana_root_path.join(format!("docker-build/{validator_type}-{index}"))
            }
        };

        self.create_base_image(
            solana_root_path,
            docker_image,
            &docker_path,
            &validator_type,
        )?;

        Ok(())
    }

    fn create_base_image(
        &self,
        solana_root_path: &Path,
        docker_image: &DockerImage,
        docker_path: &PathBuf,
        validator_type: &ValidatorType,
    ) -> Result<(), Box<dyn Error>> {
        self.create_dockerfile(validator_type, docker_path, solana_root_path, None)?;

        // We use std::process::Command here because Docker-rs is very slow building dockerfiles
        // when they are in large repos. Docker-rs doesn't seem to support the `--file` flag natively.
        // so we result to using std::process::Command
        let dockerfile = docker_path.join("Dockerfile");
        let context_path = solana_root_path.display().to_string();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{BUILD}Building {validator_type} docker image...",));

        let command = format!(
            "docker build -t {docker_image} -f {} {context_path}",
            dockerfile.display()
        );

        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to execute command")
            .wait_with_output()
            .map_err(Box::new)?;

        if !output.status.success() {
            return Err(output.status.to_string().into());
        }
        progress_bar.finish_and_clear();
        info!("{validator_type} image build complete");

        Ok(())
    }

    fn write_startup_script_to_docker_directory(
        file_name: &str,
        docker_dir: &Path,
        validator_type: &ValidatorType,
    ) -> Result<(), Box<dyn Error>> {
        let script_path = docker_dir.join(file_name);
        let script_content = validator_type.script();
        StartupScripts::write_script_to_file(script_content, &script_path).map_err(|e| e.into())
    }

    fn create_dockerfile(
        &self,
        validator_type: &ValidatorType,
        docker_path: &PathBuf,
        solana_root_path: &Path,
        content: Option<&str>,
    ) -> Result<(), Box<dyn Error>> {
        if docker_path.exists() {
            fs::remove_dir_all(docker_path)?;
        }
        fs::create_dir_all(docker_path)?;

        let file_name = format!("{validator_type}-startup-script.sh");
        Self::write_startup_script_to_docker_directory(&file_name, docker_path, validator_type)?;
        StartupScripts::write_script_to_file(
            StartupScripts::common(),
            &docker_path.join("common.sh"),
        )?;

        let startup_script_directory = match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard | ValidatorType::RPC => {
                format!("./docker-build/{validator_type}")
            }
            ValidatorType::Client(index) => format!("./docker-build/{validator_type}-{index}"),
        };
        let solana_build_directory = if let DeployMethod::ReleaseChannel(_) = self.deploy_method {
            "solana-release"
        } else {
            "farf"
        };

        let dockerfile = format!(
            r#"
FROM {}
RUN apt-get update && apt-get install -y iputils-ping curl vim && \
    rm -rf /var/lib/apt/lists/* && \
    useradd -ms /bin/bash solana && \
    adduser solana sudo

USER solana
COPY --chown=solana:solana  {startup_script_directory} /home/solana/k8s-cluster-scripts
RUN chmod +x /home/solana/k8s-cluster-scripts/*
COPY --chown=solana:solana ./config-k8s/bootstrap-validator  /home/solana/ledger
COPY --chown=solana:solana ./{solana_build_directory}/bin/ /home/solana/.cargo/bin/
COPY --chown=solana:solana ./{solana_build_directory}/version.yml /home/solana/
ENV PATH="/home/solana/.cargo/bin:${{PATH}}"

WORKDIR /home/solana
"#,
            self.base_image
        );

        let dockerfile = format!(
            "{dockerfile}\n{}",
            self.insert_client_accounts_if_present(solana_root_path, validator_type)?,
        );

        debug!("dockerfile: {dockerfile:?}");
        std::fs::write(
            docker_path.join("Dockerfile"),
            content.unwrap_or(dockerfile.as_str()),
        )?;
        Ok(())
    }

    fn insert_client_accounts_if_present(
        &self,
        solana_root_path: &Path,
        validator_type: &ValidatorType,
    ) -> Result<String, Box<dyn Error>> {
        match validator_type {
            ValidatorType::Client(index) => {
                let bench_tps_path =
                    solana_root_path.join(format!("config-k8s/bench-tps-{index}.yml"));
                if bench_tps_path.exists() {
                    Ok(format!(
                        r#"
COPY --chown=solana:solana ./config-k8s/bench-tps-{index}.yml /home/solana/client-accounts.yml
                    "#,
                    ))
                } else {
                    Err(format!("{:?} does not exist!", bench_tps_path).into())
                }
            }
            ValidatorType::Bootstrap => {
                let client_accounts_path = solana_root_path.join("config-k8s/client-accounts.yml");
                if client_accounts_path.exists() {
                    Ok(r#"
COPY --chown=solana:solana ./config-k8s/client-accounts.yml /home/solana
                    "#
                    .to_string())
                } else {
                    Err(format!("{:?} does not exist!", client_accounts_path).into())
                }
            }
            ValidatorType::Standard | ValidatorType::RPC => Ok("".to_string()),
        }
    }

    pub fn push_image(docker_image: &DockerImage) -> Result<Child, Box<dyn Error>> {
        let command = format!("docker push '{docker_image}'");
        let child = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        Ok(child)
    }

    pub fn push_images<'a, I>(&self, validators: I) -> Result<(), Box<dyn Error>>
    where
        I: IntoIterator<Item = &'a Validator>,
    {
        info!("Pushing images...");
        let children: Result<Vec<Child>, _> = validators
            .into_iter()
            .map(|validator| Self::push_image(validator.image()))
            .collect();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{ROCKET}Pushing images to registry..."));
        for child in children? {
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(output.status.to_string().into());
            }
        }
        progress_bar.finish_and_clear();

        Ok(())
    }
}
