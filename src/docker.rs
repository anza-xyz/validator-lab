use {
    crate::{
        new_spinner_progress_bar, release::DeployMethod, validator::Validator,
        DockerPushThreadError, ValidatorType, BUILD, ROCKET,
    },
    log::*,
    rayon::prelude::*,
    std::{
        env,
        error::Error,
        fmt::{self, Display, Formatter},
        fs,
        path::{Path, PathBuf},
        process::{Command, Stdio},
    },
};

#[derive(Clone)]
pub struct DockerImage {
    registry: String,
    validator_type: ValidatorType,
    image_name: String,
    tag: String,
    client_index: Option<usize>,
}

impl DockerImage {
    // Constructor to create a new instance of DockerImage
    pub fn new(
        registry: String,
        validator_type: ValidatorType,
        image_name: String,
        tag: String,
        client_index: Option<usize>,
    ) -> Self {
        DockerImage {
            registry,
            validator_type,
            image_name,
            tag,
            client_index,
        }
    }

    pub fn validator_type(&self) -> ValidatorType {
        self.validator_type
    }
}

// Put DockerImage in format for building, pushing, and pulling
impl Display for DockerImage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.client_index {
            Some(index) => write!(
                f,
                "{}/{}-{}-{}:{}",
                self.registry, self.validator_type, index, self.image_name, self.tag
            ),
            None => write!(
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
        solana_root_path: &PathBuf,
        docker_image: &DockerImage,
    ) -> Result<(), Box<dyn Error>> {
        let validator_type = docker_image.validator_type();
        match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard | ValidatorType::RPC => (),
            ValidatorType::Client => {
                return Err(format!(
                    "Build docker image for validator type: {validator_type} not supported yet"
                )
                .into());
            }
        }

        let docker_path = solana_root_path.join(format!("docker-build/{validator_type}"));
        self.create_base_image(
            solana_root_path,
            &docker_image,
            docker_path,
            &validator_type,
            None,
        )?;

        Ok(())
    }

    fn create_base_image(
        &self,
        solana_root_path: &PathBuf,
        docker_image: &DockerImage,
        docker_path: PathBuf,
        validator_type: &ValidatorType,
        index: Option<usize>,
    ) -> Result<(), Box<dyn Error>> {
        let dockerfile_path =
            self.create_dockerfile(validator_type, docker_path, solana_root_path, None, index)?;

        // We use std::process::Command here because Docker-rs is very slow building dockerfiles
        // when they are in large repos. Docker-rs doesn't seem to support the `--file` flag natively.
        // so we result to using std::process::Command
        let dockerfile = dockerfile_path.join("Dockerfile");
        let context_path = solana_root_path.display().to_string();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{BUILD}Building {validator_type} docker image..."));

        let command = format!("docker build -t {docker_image} -f {dockerfile:?} {context_path}");

        let output = match Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to execute command")
            .wait_with_output()
        {
            Ok(res) => Ok(res),
            Err(err) => Err(Box::new(err) as Box<dyn Error>),
        }?;

        if !output.status.success() {
            return Err(output.status.to_string().into());
        }
        progress_bar.finish_and_clear();

        Ok(())
    }

    fn copy_file_to_docker(
        source_dir: &Path,
        docker_dir: &Path,
        file_name: &str,
    ) -> std::io::Result<()> {
        let source_path = source_dir.join("src/scripts").join(file_name);
        let destination_path = docker_dir.join(file_name);
        fs::copy(&source_path, &destination_path)?;
        Ok(())
    }

    fn create_dockerfile(
        &self,
        validator_type: &ValidatorType,
        docker_path: PathBuf,
        solana_root_path: &PathBuf,
        content: Option<&str>,
        index: Option<usize>,
    ) -> Result<PathBuf, Box<dyn Error>> {
        if docker_path.exists() {
            fs::remove_dir_all(&docker_path)?;
        }
        fs::create_dir_all(&docker_path)?;

        if let DeployMethod::Local(_) = self.deploy_method {
            // Copy startup scripts into build directory
            let manifest_path =
                PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("$CARGO_MANIFEST_DIR"));
            let files_to_copy = [
                format!("{validator_type}-startup-script.sh"),
                "common.sh".to_string(),
            ];
            for file_name in files_to_copy.iter() {
                Self::copy_file_to_docker(&manifest_path, &docker_path, file_name)?;
            }
        }

        let solana_build_directory = match self.deploy_method {
            DeployMethod::ReleaseChannel(_) => "solana-release",
            DeployMethod::Local(_) => "farf",
        };

        let startup_script_directory = match self.deploy_method {
            DeployMethod::ReleaseChannel(_) => "./src/scripts".to_string(),
            DeployMethod::Local(_) => match validator_type {
                ValidatorType::Bootstrap | ValidatorType::RPC | ValidatorType::Standard => {
                    format!("./docker-build/{validator_type}")
                }
                ValidatorType::Client => {
                    if index.is_some() {
                        format!("./docker-build/client-{}", index.unwrap())
                    } else {
                        return Err("Error! Client index is None".into());
                    }
                }
            },
        };

        let dockerfile = format!(
            r#"
FROM {}
RUN apt-get update
RUN apt-get install -y iputils-ping curl vim bzip2

RUN useradd -ms /bin/bash solana
RUN adduser solana sudo
USER solana

RUN mkdir -p /home/solana/k8s-cluster-scripts
# TODO: this needs to be changed for non bootstrap, this should be ./src/scripts/<validator-type>-startup-scripts.sh
COPY {startup_script_directory} /home/solana/k8s-cluster-scripts
 
RUN mkdir -p /home/solana/ledger
COPY --chown=solana:solana ./config-k8s/bootstrap-validator  /home/solana/ledger

RUN mkdir -p /home/solana/.cargo/bin

COPY ./{solana_build_directory}/bin/ /home/solana/.cargo/bin/
COPY ./{solana_build_directory}/version.yml /home/solana/

RUN mkdir -p /home/solana/config
ENV PATH="/home/solana/.cargo/bin:${{PATH}}"

WORKDIR /home/solana

"#,
            self.base_image
        );

        let dockerfile = format!(
            "{dockerfile}\n{}",
            self.insert_client_accounts_if_present(solana_root_path, index)
        );

        debug!("dockerfile: {}", dockerfile);
        std::fs::write(
            docker_path.join("Dockerfile"),
            content.unwrap_or(dockerfile.as_str()),
        )?;
        Ok(docker_path)
    }

    // TODO: don't think validator and rpcs need client-accounts.yml. think only bootstrap
    fn insert_client_accounts_if_present(
        &self,
        solana_root_path: &PathBuf,
        index: Option<usize>,
    ) -> String {
        match index {
            Some(i) => {
                // client image
                if solana_root_path
                    .join(format!("config-k8s/bench-tps-{i}.yml"))
                    .exists()
                {
                    format!(
                        r#"
        COPY --chown=solana:solana ./config-k8s/bench-tps-{i}.yml /home/solana/client-accounts.yml
                    "#,
                    )
                } else {
                    "".to_string()
                }
            }
            None => {
                // bootstrap image
                if solana_root_path
                    .join("config-k8s/client-accounts.yml")
                    .exists()
                {
                    r#"
        COPY --chown=solana:solana ./config-k8s/client-accounts.yml /home/solana
                    "#
                    .to_string()
                } else {
                    "".to_string()
                }
            }
        }
    }

    pub fn push_image(docker_image: &DockerImage) -> Result<(), Box<dyn Error + Send>> {
        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!(
            "{ROCKET}Pushing {docker_image} image to registry...",
        ));
        let command = format!("docker push '{docker_image}'");
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to execute command")
            .wait_with_output()
            .expect("Failed to push image");

        if !output.status.success() {
            return Err(Box::new(DockerPushThreadError::from(
                output.status.to_string(),
            )));
        }
        progress_bar.finish_and_clear();
        Ok(())
    }

    pub fn build_client_image(
        &self,
        solana_root_path: &PathBuf,
        docker_image: &DockerImage,
        client_index: usize,
    ) -> Result<(), Box<dyn Error>> {
        let docker_path = solana_root_path.join(format!(
            "docker-build/{}-{client_index}",
            docker_image.validator_type(),
        ));
        self.create_base_image(
            solana_root_path,
            docker_image,
            docker_path,
            &ValidatorType::Client,
            Some(client_index),
        )?;
        Ok(())
    }

    pub fn push_images(&self, validators: &[&Validator]) -> Result<(), Box<dyn Error + Send>> {
        info!("Pushing images...");
        validators
            .par_iter()
            .try_for_each(|validator| Self::push_image(validator.image()))
    }

    // need a new image for each client
    pub fn push_client_images(&self, clients: &[Validator]) -> Result<(), Box<dyn Error + Send>> {
        info!("Pushing client images...");
        clients
            .par_iter()
            .try_for_each(|client| Self::push_image(client.image()))
    }
}
