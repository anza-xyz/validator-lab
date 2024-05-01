use {
    crate::{
        docker::DockerImage,
        k8s_helpers::{self, SecretType},
        validator_config::ValidatorConfig,
        Metrics, ValidatorType,
    },
    k8s_openapi::{
        api::{
            apps::v1::ReplicaSet,
            core::v1::{
                EnvVar, EnvVarSource, Namespace, ObjectFieldSelector, Secret, SecretKeySelector,
                SecretVolumeSource, Service, Volume, VolumeMount,
            },
        },
        apimachinery::pkg::api::resource::Quantity,
    },
    kube::{
        api::{Api, ListParams, PostParams},
        Client,
    },
    log::*,
    solana_sdk::pubkey::Pubkey,
    std::{collections::BTreeMap, error::Error, path::Path},
};

#[derive(Debug, Clone)]
pub struct PodRequests {
    requests: BTreeMap<String, Quantity>,
}

impl PodRequests {
    pub fn new(cpu_requests: String, memory_requests: String) -> PodRequests {
        PodRequests {
            requests: BTreeMap::from([
                ("cpu".to_string(), Quantity(cpu_requests)),
                ("memory".to_string(), Quantity(memory_requests)),
            ]),
        }
    }
}

pub struct Kubernetes<'a> {
    k8s_client: Client,
    namespace: String,
    validator_config: &'a mut ValidatorConfig,
    pod_requests: PodRequests,
    pub metrics: Option<Metrics>,
}

impl<'a> Kubernetes<'a> {
    pub async fn new(
        namespace: &str,
        validator_config: &'a mut ValidatorConfig,
        pod_requests: PodRequests,
        metrics: Option<Metrics>,
    ) -> Kubernetes<'a> {
        Self {
            k8s_client: Client::try_default().await.unwrap(),
            namespace: namespace.to_owned(),
            validator_config,
            pod_requests,
            metrics,
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
        &mut self,
        secret_name: &str,
        config_dir: &Path,
    ) -> Result<Secret, Box<dyn Error>> {
        let faucet_key_path = config_dir.join("faucet.json");
        let identity_key_path = config_dir.join("bootstrap-validator/identity.json");
        let vote_key_path = config_dir.join("bootstrap-validator/vote-account.json");
        let stake_key_path = config_dir.join("bootstrap-validator/stake-account.json");

        let mut secrets = BTreeMap::new();
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

        k8s_helpers::create_secret(secret_name.to_string(), secrets)
    }

    pub fn add_known_validator(&mut self, pubkey: Pubkey) {
        self.validator_config.known_validators.push(pubkey);
        info!("pubkey added to known validators: {:?}", pubkey);
    }

    pub async fn deploy_secret(&self, secret: &Secret) -> Result<Secret, kube::Error> {
        let secrets_api: Api<Secret> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());
        secrets_api.create(&PostParams::default(), secret).await
    }

    pub fn create_bootstrap_validator_replica_set(
        &mut self,
        image_name: &DockerImage,
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

        if self.metrics.is_some() {
            env_vars.push(self.get_metrics_env_var_secret())
        }

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

        k8s_helpers::create_replica_set(
            ValidatorType::Bootstrap,
            self.namespace.clone(),
            label_selector.clone(),
            image_name.clone(),
            env_vars,
            command.clone(),
            accounts_volume,
            accounts_volume_mount,
            self.pod_requests.requests.clone(),
            None,
        )
    }

    fn generate_command_flags(&self, flags: &mut Vec<String>) {
        if self.validator_config.skip_poh_verify {
            flags.push("--skip-poh-verify".to_string());
        }
        if self.validator_config.no_snapshot_fetch {
            flags.push("--no-snapshot-fetch".to_string());
        }
        if self.validator_config.require_tower {
            flags.push("--require-tower".to_string());
        }
        if self.validator_config.enable_full_rpc {
            flags.push("--enable-rpc-transaction-history".to_string());
            flags.push("--enable-extended-tx-metadata-storage".to_string());
        }

        if let Some(limit_ledger_size) = self.validator_config.max_ledger_size {
            flags.push("--limit-ledger-size".to_string());
            flags.push(limit_ledger_size.to_string());
        }
    }

    fn generate_bootstrap_command_flags(&self) -> Vec<String> {
        let mut flags: Vec<String> = Vec::new();
        self.generate_command_flags(&mut flags);

        flags
    }

    pub fn create_selector(&self, key: &str, value: &str) -> BTreeMap<String, String> {
        k8s_helpers::create_selector(key, value)
    }

    pub async fn deploy_replicas_set(
        &self,
        replica_set: &ReplicaSet,
    ) -> Result<ReplicaSet, kube::Error> {
        let api: Api<ReplicaSet> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());
        let post_params = PostParams::default();
        // Apply the ReplicaSet
        api.create(&post_params, replica_set).await
    }

    pub fn create_bootstrap_service(
        &self,
        service_name: &str,
        label_selector: &BTreeMap<String, String>,
    ) -> Service {
        k8s_helpers::create_service(
            service_name.to_string(),
            self.namespace.clone(),
            label_selector.clone(),
            false,
        )
    }

    pub async fn deploy_service(&self, service: &Service) -> Result<Service, kube::Error> {
        let post_params = PostParams::default();
        // Create an API instance for Services in the specified namespace
        let service_api: Api<Service> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());

        // Create the Service object in the cluster
        service_api.create(&post_params, service).await
    }

    pub fn create_validator_load_balancer(
        &self,
        service_name: &str,
        label_selector: &BTreeMap<String, String>,
    ) -> Service {
        k8s_helpers::create_service(
            service_name.to_string(),
            self.namespace.clone(),
            label_selector.clone(),
            true,
        )
    }

    pub async fn is_replica_set_ready(&self, replica_set_name: &str) -> Result<bool, kube::Error> {
        let replica_sets: Api<ReplicaSet> =
            Api::namespaced(self.k8s_client.clone(), self.namespace.as_str());
        let replica_set = replica_sets.get(replica_set_name).await?;

        let desired_validators = replica_set.spec.as_ref().unwrap().replicas.unwrap_or(1);
        let available_validators = replica_set
            .status
            .as_ref()
            .unwrap()
            .available_replicas
            .unwrap_or(0);

        Ok(available_validators >= desired_validators)
    }

    pub fn create_metrics_secret(&self) -> Result<Secret, Box<dyn std::error::Error>> {
        let mut data = BTreeMap::new();
        if let Some(metrics) = &self.metrics {
            data.insert(
                "SOLANA_METRICS_CONFIG".to_string(),
                SecretType::Value {
                    v: metrics.to_env_string(),
                },
            );
        } else {
            return Err(
                "Called create_metrics_secret() but metrics were not provided."
                    .to_string()
                    .into(),
            );
        }

        k8s_helpers::create_secret("solana-metrics-secret".to_string(), data)
    }

    pub fn get_metrics_env_var_secret(&self) -> EnvVar {
        EnvVar {
            name: "SOLANA_METRICS_CONFIG".to_string(),
            value_from: Some(EnvVarSource {
                secret_key_ref: Some(SecretKeySelector {
                    name: Some("solana-metrics-secret".to_string()),
                    key: "SOLANA_METRICS_CONFIG".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }
}
