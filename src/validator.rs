use {
    crate::{docker::DockerImage, ValidatorType},
    k8s_openapi::api::{apps::v1::ReplicaSet, core::v1::Secret},
    std::{collections::BTreeMap, string::String},
};

pub struct Validator {
    validator_type: ValidatorType,
    image: DockerImage,
    secret: Secret,
    replica_set_labels: BTreeMap<String, String>,
    replica_set: ReplicaSet,
}

impl Validator {
    pub fn new(image: DockerImage) -> Self {
        Self {
            validator_type: image.validator_type(),
            image,
            secret: Secret::default(),
            replica_set_labels: BTreeMap::new(),
            replica_set: ReplicaSet::default(),
        }
    }

    pub fn image(&self) -> &DockerImage {
        &self.image
    }

    pub fn secret(&self) -> &Secret {
        &self.secret
    }

    pub fn validator_type(&self) -> &ValidatorType {
        &self.validator_type
    }

    pub fn add_label<K, V>(&mut self, key: K, value: V)
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.replica_set_labels.insert(key.into(), value.into());
    }

    pub fn labels(&self) -> &BTreeMap<String, String> {
        &self.replica_set_labels
    }

    pub fn set_secret(&mut self, secret: Secret) {
        self.secret = secret;
    }

    pub fn set_replica_set(&mut self, replica_set: ReplicaSet) {
        self.replica_set = replica_set;
    }

    pub fn replica_set(&self) -> &ReplicaSet {
        &self.replica_set
    }

    pub fn replica_set_name(&self) -> &String {
        self.replica_set.metadata.name.as_ref().unwrap()
    }
}
