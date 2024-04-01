use {
    crate::k8s_helpers,
    k8s_openapi::api::core::v1::{Namespace, Secret},
    kube::{
        api::{Api, ListParams, PostParams},
        Client,
    },
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

    pub fn create_selector(&self, key: &str, value: &str) -> BTreeMap<String, String> {
        k8s_helpers::create_selector(key, value)
    }
}
