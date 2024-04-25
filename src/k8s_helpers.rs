use {
    crate::{docker::DockerImage, ValidatorType},
    k8s_openapi::{
        api::{
            apps::v1::{ReplicaSet, ReplicaSetSpec},
            core::v1::{
                Container, EnvVar, PodSecurityContext, PodSpec, PodTemplateSpec, Probe,
                ResourceRequirements, Secret, Volume, VolumeMount,
            },
        },
        apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::LabelSelector},
        ByteString,
    },
    kube::api::ObjectMeta,
    std::{collections::BTreeMap, error::Error, path::PathBuf},
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
    secrets: BTreeMap<String, SecretType>,
) -> Result<Secret, Box<dyn Error>> {
    let data = secrets
        .into_iter()
        .map(|(label, value)| match value {
            SecretType::Value { v } => Ok((label, ByteString(v.into_bytes()))),
            SecretType::File { path } => {
                let content = std::fs::read(&path)
                    .map_err(|err| format!("Failed to read file '{:?}': {}", path, err))?;
                Ok((label, ByteString(content)))
            }
        })
        .collect::<Result<BTreeMap<String, ByteString>, Box<dyn Error>>>()?;

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
    image_name: &DockerImage,
    environment_variables: Vec<EnvVar>,
    command: &[String],
    volumes: Option<Vec<Volume>>,
    volume_mounts: Option<Vec<VolumeMount>>,
    pod_requests: BTreeMap<String, Quantity>,
    readiness_probe: Option<Probe>,
) -> Result<ReplicaSet, Box<dyn Error>> {
    let pod_spec = PodTemplateSpec {
        metadata: Some(ObjectMeta {
            labels: Some(label_selector.clone()),
            ..Default::default()
        }),
        spec: Some(PodSpec {
            containers: vec![Container {
                name: format!("{}-{}", image_name.validator_type(), "container"),
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
