use {
    crate::k8s_helpers::{self, SecretType},
    k8s_openapi::api::core::v1::{Namespace, Secret},
    kube::{
        api::{Api, ListParams, PostParams},
        Client,
    },
    std::{
        collections::{BTreeMap, HashMap},
        error::Error,
        path::Path,
    },
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
        config_dir: &Path,
    ) -> Result<Secret, Box<dyn Error>> {
        let faucet_key_path = config_dir.join("faucet.json");
        let identity_key_path = config_dir.join("bootstrap-validator/identity.json");
        let vote_key_path = config_dir.join("bootstrap-validator/vote-account.json");
        let stake_key_path = config_dir.join("bootstrap-validator/stake-account.json");

        let mut secrets = HashMap::new();
        secrets.insert(
            "faucet".to_string(),
            SecretType::File {
                path: faucet_key_path,
            },
        );
        secrets.insert(
            "identity".to_string(),
            SecretType::File {
                path: identity_key_path,
            },
        );
        secrets.insert(
            "vote".to_string(),
            SecretType::File {
                path: vote_key_path,
            },
        );
        secrets.insert(
            "stake".to_string(),
            SecretType::File {
                path: stake_key_path,
            },
        );

        k8s_helpers::create_secret(secret_name, secrets)
    }

    pub async fn deploy_secret(&self, secret: &Secret) -> Result<Secret, kube::Error> {
        let secrets_api: Api<Secret> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());
        secrets_api.create(&PostParams::default(), secret).await
    }

    pub fn create_selector(&self, key: &str, value: &str) -> BTreeMap<String, String> {
        k8s_helpers::create_selector(key, value)
    }
}
