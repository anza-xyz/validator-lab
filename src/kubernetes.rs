use {
    crate::{
        client_config::ClientConfig,
        docker::DockerImage,
        k8s_helpers::{self, SecretType},
        validator_config::ValidatorConfig,
        Metrics, ValidatorType,
    },
    k8s_openapi::{
        api::{
            apps::v1::ReplicaSet,
            core::v1::{
                EnvVar, EnvVarSource, ExecAction, Namespace, ObjectFieldSelector, Probe, Secret,
                SecretKeySelector, SecretVolumeSource, Service, Volume, VolumeMount,
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
    client_config: ClientConfig,
    pod_requests: PodRequests,
    pub metrics: Option<Metrics>,
}

impl<'a> Kubernetes<'a> {
    pub async fn new(
        namespace: &str,
        validator_config: &'a mut ValidatorConfig,
        client_config: ClientConfig,
        pod_requests: PodRequests,
        metrics: Option<Metrics>,
    ) -> Kubernetes<'a> {
        Self {
            k8s_client: Client::try_default().await.unwrap(),
            namespace: namespace.to_owned(),
            validator_config,
            client_config,
            pod_requests,
            metrics,
        }
    }

    pub fn set_shred_version(&mut self, shred_version: u16) {
        self.validator_config.shred_version = Some(shred_version);
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

    pub fn create_validator_secret(
        &self,
        validator_index: usize,
        config_dir: &Path,
    ) -> Result<Secret, Box<dyn Error>> {
        let secret_name = format!("validator-accounts-secret-{validator_index}");

        let mut secrets = BTreeMap::new();
        secrets.insert(
            "identity".to_string(),
            SecretType::File {
                path: config_dir.join(format!("validator-identity-{validator_index}.json")),
            },
        );

        let secret_types = ["vote", "stake"];
        for &type_name in secret_types.iter() {
            secrets.insert(
                type_name.to_string(),
                SecretType::File {
                    path: config_dir.join(format!(
                        "validator-{type_name}-account-{validator_index}.json"
                    )),
                },
            );
        }

        k8s_helpers::create_secret(secret_name.to_string(), secrets)
    }

    pub fn create_rpc_secret(
        &self,
        rpc_index: usize,
        config_dir: &Path,
    ) -> Result<Secret, Box<dyn Error>> {
        let secret_name = format!("rpc-node-account-secret-{rpc_index}");

        let mut secrets = BTreeMap::new();
        secrets.insert(
            "identity".to_string(),
            SecretType::File {
                path: config_dir.join(format!("rpc-node-identity-{rpc_index}.json")),
            },
        );
        secrets.insert(
            "faucet".to_string(),
            SecretType::File {
                path: config_dir.join("faucet.json"),
            },
        );

        k8s_helpers::create_secret(secret_name, secrets)
    }

    pub fn create_client_secret(
        &self,
        client_index: usize,
        config_dir: &Path,
    ) -> Result<Secret, Box<dyn Error>> {
        let secret_name = format!("client-accounts-secret-{client_index}");
        let faucet_key_path = config_dir.join("faucet.json");
        let identity_key_path = config_dir.join(format!("validator-identity-{}.json", 0));

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

        k8s_helpers::create_secret(secret_name, secrets)
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

        let command_path = format!(
            "/home/solana/k8s-cluster-scripts/{}-startup-script.sh",
            ValidatorType::Bootstrap
        );
        let mut command = vec![command_path];
        command.extend(self.generate_bootstrap_command_flags());

        k8s_helpers::create_replica_set(
            ValidatorType::Bootstrap.to_string(),
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

    fn generate_client_command_flags(&self) -> Vec<String> {
        let mut flags = vec![];

        flags.push(self.client_config.client_to_run.clone()); //client to run
        if !self.client_config.bench_tps_args.is_empty() {
            flags.push(self.client_config.bench_tps_args.join(" "));
        }

        flags.push(self.client_config.client_type.clone());

        if let Some(target_node) = self.client_config.target_node {
            flags.push("--target-node".to_string());
            flags.push(target_node.to_string());
        }

        flags.push("--duration".to_string());
        flags.push(self.client_config.duration.to_string());

        if let Some(num_nodes) = self.client_config.num_nodes {
            flags.push("--num-nodes".to_string());
            flags.push(num_nodes.to_string());
        }

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

    pub fn create_service(
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

    fn set_non_bootstrap_environment_variables(&self) -> Vec<EnvVar> {
        vec![
            k8s_helpers::create_environment_variable(
                "NAMESPACE".to_string(),
                None,
                Some("metadata.namespace".to_string()),
            ),
            k8s_helpers::create_environment_variable(
                "BOOTSTRAP_RPC_ADDRESS".to_string(),
                Some("bootstrap-validator-service.$(NAMESPACE).svc.cluster.local:8899".to_string()),
                None,
            ),
            k8s_helpers::create_environment_variable(
                "BOOTSTRAP_GOSSIP_ADDRESS".to_string(),
                Some("bootstrap-validator-service.$(NAMESPACE).svc.cluster.local:8001".to_string()),
                None,
            ),
            k8s_helpers::create_environment_variable(
                "BOOTSTRAP_FAUCET_ADDRESS".to_string(),
                Some("bootstrap-validator-service.$(NAMESPACE).svc.cluster.local:9900".to_string()),
                None,
            ),
        ]
    }

    fn set_load_balancer_environment_variables(&self) -> Vec<EnvVar> {
        vec![
            k8s_helpers::create_environment_variable(
                "LOAD_BALANCER_RPC_ADDRESS".to_string(),
                Some(
                    "bootstrap-and-rpc-node-lb-service.$(NAMESPACE).svc.cluster.local:8899"
                        .to_string(),
                ),
                None,
            ),
            k8s_helpers::create_environment_variable(
                "LOAD_BALANCER_GOSSIP_ADDRESS".to_string(),
                Some(
                    "bootstrap-and-rpc-node-lb-service.$(NAMESPACE).svc.cluster.local:8001"
                        .to_string(),
                ),
                None,
            ),
            k8s_helpers::create_environment_variable(
                "LOAD_BALANCER_FAUCET_ADDRESS".to_string(),
                Some(
                    "bootstrap-and-rpc-node-lb-service.$(NAMESPACE).svc.cluster.local:9900"
                        .to_string(),
                ),
                None,
            ),
        ]
    }

    fn add_known_validators_if_exists(&self, flags: &mut Vec<String>) {
        for key in self.validator_config.known_validators.iter() {
            flags.push("--known-validator".to_string());
            flags.push(key.to_string());
        }
    }

    fn generate_validator_command_flags(&self) -> Vec<String> {
        let mut flags: Vec<String> = Vec::new();
        self.generate_command_flags(&mut flags);

        flags.push("--internal-node-stake-sol".to_string());
        flags.push(self.validator_config.internal_node_stake_sol.to_string());

        flags.push("--internal-node-sol".to_string());
        flags.push(self.validator_config.internal_node_sol.to_string());

        if let Some(shred_version) = self.validator_config.shred_version {
            flags.push("--expected-shred-version".to_string());
            flags.push(shred_version.to_string());
        }

        self.add_known_validators_if_exists(&mut flags);

        flags
    }

    pub fn create_validator_replica_set(
        &mut self,
        image: &DockerImage,
        secret_name: Option<String>,
        label_selector: &BTreeMap<String, String>,
        validator_index: usize,
    ) -> Result<ReplicaSet, Box<dyn Error>> {
        let mut env_vars = self.set_non_bootstrap_environment_variables();
        if self.metrics.is_some() {
            env_vars.push(self.get_metrics_env_var_secret())
        }
        env_vars.append(&mut self.set_load_balancer_environment_variables());

        let accounts_volume = Some(vec![Volume {
            name: format!("validator-accounts-volume-{validator_index}"),
            secret: Some(SecretVolumeSource {
                secret_name,
                ..Default::default()
            }),
            ..Default::default()
        }]);

        let accounts_volume_mount = Some(vec![VolumeMount {
            name: format!("validator-accounts-volume-{validator_index}"),
            mount_path: "/home/solana/validator-accounts".to_string(),
            ..Default::default()
        }]);

        let mut command =
            vec!["/home/solana/k8s-cluster-scripts/validator-startup-script.sh".to_string()];
        command.extend(self.generate_validator_command_flags());

        k8s_helpers::create_replica_set(
            format!("{}-{validator_index}", ValidatorType::Standard),
            self.namespace.clone(),
            label_selector.clone(),
            image.clone(),
            env_vars,
            command.clone(),
            accounts_volume,
            accounts_volume_mount,
            self.pod_requests.requests.clone(),
            None,
        )
    }

    fn generate_rpc_command_flags(&self) -> Vec<String> {
        let mut flags: Vec<String> = Vec::new();
        self.generate_command_flags(&mut flags);
        if let Some(shred_version) = self.validator_config.shred_version {
            flags.push("--expected-shred-version".to_string());
            flags.push(shred_version.to_string());
        }

        self.add_known_validators_if_exists(&mut flags);

        flags
    }

    pub fn create_rpc_replica_set(
        &mut self,
        image: &DockerImage,
        secret_name: Option<String>,
        label_selector: &BTreeMap<String, String>,
        rpc_index: usize,
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
        env_vars.append(&mut self.set_non_bootstrap_environment_variables());
        env_vars.append(&mut self.set_load_balancer_environment_variables());

        if self.metrics.is_some() {
            env_vars.push(self.get_metrics_env_var_secret())
        }

        let accounts_volume = Some(vec![Volume {
            name: format!("rpc-node-accounts-volume-{}", rpc_index),
            secret: Some(SecretVolumeSource {
                secret_name,
                ..Default::default()
            }),
            ..Default::default()
        }]);

        let accounts_volume_mount = Some(vec![VolumeMount {
            name: format!("rpc-node-accounts-volume-{}", rpc_index),
            mount_path: "/home/solana/rpc-node-accounts".to_string(),
            ..Default::default()
        }]);

        let mut command =
            vec!["/home/solana/k8s-cluster-scripts/rpc-node-startup-script.sh".to_string()];
        command.extend(self.generate_rpc_command_flags());

        let exec_action = ExecAction {
            command: Some(vec![
                String::from("/bin/bash"),
                String::from("-c"),
                String::from(
                    "solana -u http://$MY_POD_IP:8899 balance -k rpc-node-accounts/identity.json",
                ),
            ]),
        };

        let readiness_probe = Probe {
            exec: Some(exec_action),
            initial_delay_seconds: Some(20),
            period_seconds: Some(20),
            ..Default::default()
        };

        k8s_helpers::create_replica_set(
            format!("{}-{rpc_index}", ValidatorType::RPC),
            self.namespace.clone(),
            label_selector.clone(),
            image.clone(),
            env_vars,
            command.clone(),
            accounts_volume,
            accounts_volume_mount,
            self.pod_requests.requests.clone(),
            Some(readiness_probe),
        )
    }

    pub fn create_client_replica_set(
        &mut self,
        image: &DockerImage,
        secret_name: Option<String>,
        label_selector: &BTreeMap<String, String>,
        client_index: usize,
    ) -> Result<ReplicaSet, Box<dyn Error>> {
        let mut env_vars = self.set_non_bootstrap_environment_variables();
        if self.metrics.is_some() {
            env_vars.push(self.get_metrics_env_var_secret())
        }
        env_vars.append(&mut self.set_load_balancer_environment_variables());

        let accounts_volume = Some(vec![Volume {
            name: format!("client-accounts-volume-{}", client_index),
            secret: Some(SecretVolumeSource {
                secret_name,
                ..Default::default()
            }),
            ..Default::default()
        }]);

        let accounts_volume_mount = Some(vec![VolumeMount {
            name: format!("client-accounts-volume-{}", client_index),
            mount_path: "/home/solana/client-accounts".to_string(),
            ..Default::default()
        }]);

        let mut command =
            vec!["/home/solana/k8s-cluster-scripts/client-startup-script.sh".to_string()];
        command.extend(self.generate_client_command_flags());

        k8s_helpers::create_replica_set(
            format!("client-{client_index}"),
            self.namespace.clone(),
            label_selector.clone(),
            image.clone(),
            env_vars,
            command.clone(),
            accounts_volume,
            accounts_volume_mount,
            self.pod_requests.requests.clone(),
            None,
        )
    }
}
