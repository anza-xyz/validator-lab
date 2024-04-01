use {
    crate::{k8s_helpers, ValidatorType},
    k8s_openapi::api::{
        apps::v1::ReplicaSet,
        core::v1::{
            EnvVar, EnvVarSource, Namespace, ObjectFieldSelector,
            Secret, SecretVolumeSource, Volume, VolumeMount,
        },
    },
    kube::{
        api::{Api, ListParams, PostParams},
        Client,
    },
    log::*,
    std::{collections::BTreeMap, error::Error, path::PathBuf},
};

pub struct Kubernetes {
    k8s_client: Client,
    namespace: String,
}

impl Kubernetes {
    pub async fn new(namespace: &str) -> Kubernetes {
        Self {
            k8s_client: Client::try_default().await.unwrap(),
            namespace: namespace.to_owned(),
        }
    }

    pub async fn namespace_exists(&self) -> Result<bool, kube::Error> {
        let namespaces: Api<Namespace> = Api::all(self.k8s_client.clone());
        let namespace_list = namespaces.list(&ListParams::default()).await?;

        let exists = namespace_list
            .items
            .iter()
            .any(|ns| ns.metadata.name.as_ref() == Some(&self.namespace));

        Ok(exists)
    }

    pub fn create_bootstrap_secret(
        &self,
        secret_name: &str,
        config_dir: &PathBuf,
    ) -> Result<Secret, Box<dyn Error>> {
        let faucet_key_path = config_dir.join("faucet.json");
        let identity_key_path = config_dir.join("bootstrap-validator/identity.json");
        let vote_key_path = config_dir.join("bootstrap-validator/vote-account.json");
        let stake_key_path = config_dir.join("bootstrap-validator/stake-account.json");

        let key_files = vec![
            (faucet_key_path, "faucet"),
            (identity_key_path, "identity"),
            (vote_key_path, "vote"),
            (stake_key_path, "stake"),
        ];

        k8s_helpers::create_secret_from_files(secret_name, &key_files)
    }

    pub async fn deploy_secret(&self, secret: &Secret) -> Result<Secret, kube::Error> {
        let secrets_api: Api<Secret> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());
        secrets_api.create(&PostParams::default(), secret).await
    }

    pub fn create_bootstrap_validator_replica_set(
        &mut self,
        container_name: &str,
        image_name: &str,
        secret_name: Option<String>,
        label_selector: &BTreeMap<String, String>,
    ) -> Result<ReplicaSet, Box<dyn Error>> {
        let mut env_vars = vec![EnvVar {
            name: "MY_POD_IP".to_string(),
            value_from: Some(EnvVarSource {
                field_ref: Some(ObjectFieldSelector {
                    field_path: "status.podIP".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }];


        let accounts_volume = Some(vec![Volume {
            name: "bootstrap-accounts-volume".into(),
            secret: Some(SecretVolumeSource {
                secret_name,
                ..Default::default()
            }),
            ..Default::default()
        }]);

        let accounts_volume_mount = Some(vec![VolumeMount {
            name: "bootstrap-accounts-volume".to_string(),
            mount_path: "/home/solana/bootstrap-accounts".to_string(),
            ..Default::default()
        }]);

        let mut command =
            vec!["/home/solana/k8s-cluster-scripts/bootstrap-startup-script.sh".to_string()];
        command.extend(self.generate_bootstrap_command_flags());

        for c in command.iter() {
            debug!("bootstrap command: {}", c);
        }

        k8s_helpers::create_replica_set(
            &ValidatorType::Bootstrap,
            self.namespace.as_str(),
            label_selector,
            container_name,
            image_name,
            env_vars,
            &command,
            accounts_volume,
            accounts_volume_mount,
            None,
            self.pod_requests.requests.clone(),
        )
    }

    pub fn create_selector(&self, key: &str, value: &str) -> BTreeMap<String, String> {
        k8s_helpers::create_selector(key, value)
    }
}
