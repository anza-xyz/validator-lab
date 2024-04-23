use {
    k8s_openapi::{api::core::v1::Secret, ByteString},
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
