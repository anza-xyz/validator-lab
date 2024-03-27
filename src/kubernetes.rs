use {
    k8s_openapi::api::core::v1::Namespace,
    kube::{
        api::{Api, ListParams},
        Client,
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
}
