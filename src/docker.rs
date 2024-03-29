use {
    crate::{
        boxed_error, new_spinner_progress_bar, ValidatorType, BUILD,
    },
    log::*,
    std::{
        error::Error,
        fs,
        path::PathBuf,
        process::{Command, Output, Stdio},
    },
};

pub struct DockerConfig {
    pub base_image: String,
    pub image_name: String,
    pub tag: String,
    pub registry: String,
    deploy_method: String,
}

impl DockerConfig {
    pub fn new(
        base_image: String,
        image_name: String,
        tag: String,
        registry: String,
        deploy_method: String,
    ) -> Self {
        DockerConfig {
            base_image,
            image_name,
            tag,
            registry,
            deploy_method,
        }
    }

    pub fn build_image(&self, solana_root_path: PathBuf, validator_type: &ValidatorType) -> Result<(), Box<dyn Error>> {
        let image_name = format!("{}-{}", validator_type, self.image_name);
        let docker_path = solana_root_path.join(format!("{}/{}", "docker-build", validator_type));
        match self.create_base_image(solana_root_path, image_name, docker_path, validator_type) {
            Ok(res) => {
                if res.status.success() {
                    info!("Successfully created base Image");
                    Ok(())
                } else {
                    error!("Failed to build base image");
                    Err(boxed_error!(String::from_utf8_lossy(&res.stderr)))
                }
            }
            Err(err) => Err(err),
        }
    }

    pub fn create_base_image(
        &self,
        solana_root_path: PathBuf, 
        image_name: String,
        docker_path: PathBuf,
        validator_type: &ValidatorType,
    ) -> Result<Output, Box<dyn Error>> {
        let dockerfile_path = self.create_dockerfile(validator_type, docker_path, None)?;

        trace!("Tmp: {}", dockerfile_path.as_path().display());
        trace!("Exists: {}", dockerfile_path.as_path().exists());

        // We use std::process::Command here because Docker-rs is very slow building dockerfiles
        // when they are in large repos. Docker-rs doesn't seem to support the `--file` flag natively.
        // so we result to using std::process::Command
        let dockerfile = dockerfile_path.join("Dockerfile");
        let context_path = solana_root_path.display().to_string();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!(
            "{BUILD}Building {} docker image...",
            validator_type
        ));

        let command = format!(
            "docker build -t {}/{}:{} -f {:?} {}",
            self.registry, image_name, self.tag, dockerfile, context_path
        );
        info!("command: {}", command);
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
        };
        progress_bar.finish_and_clear();
        info!("{} image build complete", validator_type);

        output
    }

    pub fn create_dockerfile(
        &self,
        validator_type: &ValidatorType,
        docker_path: PathBuf,
        content: Option<&str>,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        match validator_type {
            ValidatorType::Bootstrap | ValidatorType::Standard | ValidatorType::RPC => (),
            _ => {
                return Err(boxed_error!(format!(
                    "Invalid validator type: {}. Exiting...",
                    validator_type
                )));
            }
        }

        if docker_path.exists() {
            fs::remove_dir_all(&docker_path)?;
        }
        fs::create_dir_all(&docker_path)?;

        let solana_build_directory = if self.deploy_method == "tar" {
            "solana-release"
        } else {
            "farf"
        };

        //TODO: I Removed some stuff from this dockerfile. may need to add some stuff back in
        let dockerfile = format!(
            r#"
FROM {}
RUN apt-get update
RUN apt-get install -y iputils-ping curl vim bzip2

RUN useradd -ms /bin/bash solana
RUN adduser solana sudo
USER solana

RUN mkdir -p /home/solana/k8s-cluster-scripts
COPY ./src/scripts /home/solana/k8s-cluster-scripts

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

        debug!("dockerfile: {}", dockerfile);
        std::fs::write(
            docker_path.as_path().join("Dockerfile"),
            content.unwrap_or(dockerfile.as_str()),
        )
        .expect("saved Dockerfile");
        Ok(docker_path)
    }
}
