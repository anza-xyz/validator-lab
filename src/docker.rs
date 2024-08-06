use {
    crate::{
        new_spinner_progress_bar, node::Node, startup_scripts::StartupScripts, ClientType,
        NodeType, BUILD, ROCKET, SOLANA_RELEASE,
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
    node_type: NodeType,
    image_name: String,
    tag: String, // commit (`abcd1234`) or version (`v1.18.12`)
    optional_full_image_path: Option<String>, // <registry>/<name>:<tag>
}

impl DockerImage {
    // Constructor to create a new instance of DockerImage
    pub fn new(registry: String, node_type: NodeType, image_name: String, tag: String) -> Self {
        DockerImage {
            registry,
            node_type,
            image_name,
            tag,
            optional_full_image_path: None,
        }
    }

    /// parse from string <registry>/<name>:<tag>
    pub fn new_from_string(image_string: String) -> Result<Self, Box<dyn Error>> {
        let split_string: Vec<&str> = image_string.split('/').collect();
        if split_string.len() != 2 {
            return Err("Invalid format. Expected <registry>/<name>:<tag>".into());
        }

        let registry = split_string[0].to_string();

        // Split the second part into name and tag
        let name_tag: Vec<&str> = split_string[1].split(':').collect();
        if name_tag.len() != 2 {
            return Err("Invalid format. Expected <registry>/<name>:<tag>".into());
        }

        Ok(DockerImage {
            registry,
            node_type: NodeType::Client(ClientType::Generic, 0),
            image_name: name_tag[0].to_string(),
            tag: name_tag[1].to_string(),
            optional_full_image_path: Some(image_string),
        })
    }

    pub fn node_type(&self) -> NodeType {
        self.node_type
    }

    pub fn tag(&self) -> String {
        self.tag.clone()
    }
}

// Put DockerImage in format for building, pushing, and pulling
impl Display for DockerImage {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.node_type {
            NodeType::Client(_, index) => {
                if let Some(image_path) = &self.optional_full_image_path {
                    write!(f, "{image_path}")
                } else {
                    write!(
                        f,
                        "{}/{}-{}-{}:{}",
                        self.registry, self.node_type, index, self.image_name, self.tag
                    )
                }
            }
            NodeType::Bootstrap | NodeType::Standard | NodeType::RPC => write!(
                f,
                "{}/{}-{}:{}",
                self.registry, self.node_type, self.image_name, self.tag
            ),
        }
    }
}

pub struct DockerConfig {
    pub base_image: String,
}

impl DockerConfig {
    pub fn new(base_image: String) -> Self {
        DockerConfig { base_image }
    }

    pub fn build_image(
        &self,
        solana_root_path: &Path,
        docker_image: &DockerImage,
    ) -> Result<(), Box<dyn Error>> {
        let node_type = docker_image.node_type();
        let docker_path = match node_type {
            NodeType::Bootstrap | NodeType::Standard | NodeType::RPC => {
                solana_root_path.join(format!("docker-build/{node_type}"))
            }
            NodeType::Client(_, index) => {
                solana_root_path.join(format!("docker-build/{node_type}-{index}"))
            }
        };

        self.create_base_image(solana_root_path, docker_image, &docker_path, &node_type)?;

        Ok(())
    }

    fn create_base_image(
        &self,
        solana_root_path: &Path,
        docker_image: &DockerImage,
        docker_path: &PathBuf,
        node_type: &NodeType,
    ) -> Result<(), Box<dyn Error>> {
        self.create_dockerfile(node_type, docker_path, solana_root_path, None)?;

        // We use std::process::Command here because Docker-rs is very slow building dockerfiles
        // when they are in large repos. Docker-rs doesn't seem to support the `--file` flag natively.
        // so we result to using std::process::Command
        let dockerfile = docker_path.join("Dockerfile");
        let context_path = solana_root_path.display().to_string();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{BUILD}Building {node_type} docker image...",));
        let command = format!(
            "docker build -t {docker_image} -f {} {context_path}",
            dockerfile.display()
        );
        debug!("docker command: {command}");

        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(Box::new)?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into());
        }
        progress_bar.finish_and_clear();

        Ok(())
    }

    fn write_startup_script_to_docker_directory(
        file_name: &str,
        docker_dir: &Path,
        node_type: &NodeType,
    ) -> Result<(), Box<dyn Error>> {
        let script_path = docker_dir.join(file_name);
        let script_content = node_type.script();
        StartupScripts::write_script_to_file(script_content, &script_path).map_err(|e| e.into())
    }

    fn create_dockerfile(
        &self,
        node_type: &NodeType,
        docker_path: &PathBuf,
        solana_root_path: &Path,
        content: Option<&str>,
    ) -> Result<(), Box<dyn Error>> {
        if docker_path.exists() {
            fs::remove_dir_all(docker_path)?;
        }
        fs::create_dir_all(docker_path)?;

        let file_name = format!("{node_type}-startup-script.sh");
        Self::write_startup_script_to_docker_directory(&file_name, docker_path, node_type)?;
        StartupScripts::write_script_to_file(
            StartupScripts::common(),
            &docker_path.join("common.sh"),
        )?;

        let startup_script_directory = match node_type {
            NodeType::Bootstrap | NodeType::Standard | NodeType::RPC => {
                format!("./docker-build/{node_type}")
            }
            NodeType::Client(_, index) => format!("./docker-build/{node_type}-{index}"),
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
{}
COPY --chown=solana:solana ./{SOLANA_RELEASE}/bin/ /home/solana/bin/
COPY --chown=solana:solana ./{SOLANA_RELEASE}/version.yml /home/solana/
ENV PATH="/home/solana/bin:${{PATH}}"

WORKDIR /home/solana
{}
"#,
            self.base_image,
            DockerConfig::check_copy_ledger(node_type),
            self.insert_client_accounts_if_present(solana_root_path, node_type)?
        );

        debug!("dockerfile: {dockerfile:?}");
        std::fs::write(
            docker_path.join("Dockerfile"),
            content.unwrap_or(dockerfile.as_str()),
        )?;
        Ok(())
    }

    fn check_copy_ledger(node_type: &NodeType) -> String {
        match node_type {
            NodeType::Bootstrap | NodeType::RPC => {
                "COPY --chown=solana:solana ./config-k8s/bootstrap-validator /home/solana/ledger"
                    .to_string()
            }
            NodeType::Standard | &NodeType::Client(_, _) => "".to_string(),
        }
    }

    fn insert_client_accounts_if_present(
        &self,
        solana_root_path: &Path,
        node_type: &NodeType,
    ) -> Result<String, Box<dyn Error>> {
        match node_type {
            NodeType::Client(_, index) => {
                let bench_tps_path =
                    solana_root_path.join(format!("config-k8s/bench-tps-{index}.yml"));
                if bench_tps_path.exists() {
                    Ok(format!(
                        r#"
COPY --chown=solana:solana ./config-k8s/bench-tps-{index}.yml /home/solana/client-accounts.yml
                    "#
                    ))
                } else {
                    Err(format!("{bench_tps_path:?} does not exist!").into())
                }
            }
            NodeType::Bootstrap | NodeType::Standard | NodeType::RPC => Ok("".to_string()),
        }
    }

    pub fn push_image(docker_image: &DockerImage) -> Result<Child, Box<dyn Error>> {
        let command = format!("docker push '{docker_image}'");
        let child = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        Ok(child)
    }

    pub fn push_images<'a, I>(&self, nodes: I) -> Result<(), Box<dyn Error>>
    where
        I: IntoIterator<Item = &'a Node>,
    {
        info!("Pushing images...");
        let children: Result<Vec<Child>, _> = nodes
            .into_iter()
            .map(|node| Self::push_image(node.image()))
            .collect();

        let progress_bar = new_spinner_progress_bar();
        progress_bar.set_message(format!("{ROCKET}Pushing images to registry..."));
        for child in children? {
            let output = child.wait_with_output()?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into());
            }
        }
        progress_bar.finish_and_clear();

        Ok(())
    }
}
