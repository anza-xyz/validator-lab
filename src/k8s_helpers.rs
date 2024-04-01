use {
    crate::ValidatorType,
    k8s_openapi::{
        api::{
            apps::v1::{ReplicaSet, ReplicaSetSpec},
            core::v1::{
                Affinity, Container, EnvVar, PodSecurityContext,
                PodSpec, PodTemplateSpec, Probe, ResourceRequirements, Secret,
                Volume, VolumeMount,
            },
        },
        apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::LabelSelector},
        ByteString,
    },
    kube::api::ObjectMeta,
    std::{
        collections::{BTreeMap, HashMap},
        error::Error,
        path::PathBuf,
    },
};

pub enum SecretType {
    Value { v: String },
    File { path: PathBuf },
}

fn build_secret(name: &str, data: BTreeMap<String, ByteString>) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        data: Some(data),
        ..Default::default()
    }
}

pub fn create_secret(
    secret_name: &str,
    secrets: HashMap<String, SecretType>,
) -> Result<Secret, Box<dyn Error>> {
    let mut data: BTreeMap<String, ByteString> = BTreeMap::new();
    for (label, value) in secrets {
        match value {
            SecretType::Value { v } => {
                data.insert(label, ByteString(v.into_bytes()));
            }
            SecretType::File { path } => {
                let file_content = std::fs::read(&path)
                    .map_err(|err| format!("Failed to read file '{:?}': {}", path, err))?;
                data.insert(label, ByteString(file_content));
            }
        }
    }
    Ok(build_secret(secret_name, data))
}

pub fn create_selector(key: &str, value: &str) -> BTreeMap<String, String> {
    let mut btree = BTreeMap::new();
    btree.insert(key.to_string(), value.to_string());
    btree
}

#[allow(clippy::too_many_arguments)]
pub fn create_replica_set(
    name: &ValidatorType,
    namespace: &str,
    label_selector: &BTreeMap<String, String>,
    container_name: &str,
    image_name: &str,
    environment_variables: Vec<EnvVar>,
    command: &[String],
    volumes: Option<Vec<Volume>>,
    volume_mounts: Option<Vec<VolumeMount>>,
    readiness_probe: Option<Probe>,
    pod_requests: BTreeMap<String, Quantity>,
) -> Result<ReplicaSet, Box<dyn Error>> {
    let pod_spec = PodTemplateSpec {
        metadata: Some(ObjectMeta {
            labels: Some(label_selector.clone()),
            ..Default::default()
        }),
        spec: Some(PodSpec {
            containers: vec![Container {
                name: container_name.to_string(),
                image: Some(image_name.to_string()),
                image_pull_policy: Some("Always".to_string()),
                env: Some(environment_variables),
                command: Some(command.to_owned()),
                volume_mounts,
                readiness_probe,
                resources: Some(ResourceRequirements {
                    requests: Some(pod_requests),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            volumes,
            security_context: Some(PodSecurityContext {
                run_as_user: Some(1000),
                run_as_group: Some(1000),
                ..Default::default()
            }),
            ..Default::default()
        }),
    };

    let replicas_set_spec = ReplicaSetSpec {
        replicas: Some(1),
        selector: LabelSelector {
            match_labels: Some(label_selector.clone()),
            ..Default::default()
        },
        template: Some(pod_spec),
        ..Default::default()
    };

    Ok(ReplicaSet {
        metadata: ObjectMeta {
            name: Some(format!("{}-replicaset", name)),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        spec: Some(replicas_set_spec),
        ..Default::default()
    })
}
