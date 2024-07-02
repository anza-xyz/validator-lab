use {
    crate::{docker::DockerImage, NodeType},
    k8s_openapi::api::{apps::v1::ReplicaSet, core::v1::Secret},
    std::{collections::BTreeMap, string::String},
};

pub enum LabelType {
    Info,
    Service,
}

#[derive(Clone)]
pub struct Node {
    node_type: NodeType,
    image: DockerImage,
    secret: Secret,
    info_labels: BTreeMap<String, String>,
    replica_set: ReplicaSet,
    service_labels: BTreeMap<String, String>,
}

impl Node {
    pub fn new(image: DockerImage) -> Self {
        Self {
            node_type: image.node_type(),
            image,
            secret: Secret::default(),
            info_labels: BTreeMap::new(),
            replica_set: ReplicaSet::default(),
            service_labels: BTreeMap::new(),
        }
    }

    pub fn image(&self) -> &DockerImage {
        &self.image
    }

    pub fn secret(&self) -> &Secret {
        &self.secret
    }

    pub fn node_type(&self) -> &NodeType {
        &self.node_type
    }

    pub fn add_label<K, V>(&mut self, key: K, value: V, label_type: LabelType)
    where
        K: Into<String>,
        V: Into<String>,
    {
        match label_type {
            LabelType::Info => {
                self.info_labels.insert(key.into(), value.into());
            }
            LabelType::Service => {
                self.service_labels.insert(key.into(), value.into());
            }
        }
    }

    pub fn info_labels(&self) -> &BTreeMap<String, String> {
        &self.info_labels
    }

    pub fn service_labels(&self) -> &BTreeMap<String, String> {
        &self.service_labels
    }

    pub fn all_labels(&self) -> BTreeMap<String, String> {
        let mut all_labels = BTreeMap::new();

        // Add all info_labels first; these can be overwritten by service_labels
        for (key, value) in &self.info_labels {
            all_labels.insert(key.clone(), value.clone());
        }

        // Add all service_labels; this will overwrite any duplicate keys from info_labels
        for (key, value) in &self.service_labels {
            all_labels.insert(key.clone(), value.clone());
        }

        all_labels
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
