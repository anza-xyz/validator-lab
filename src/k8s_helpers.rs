use {
    k8s_openapi::{api::core::v1::Secret, ByteString},
    kube::api::ObjectMeta,
    std::{collections::BTreeMap, error::Error, path::PathBuf},
};

fn create_secret(name: &str, data: BTreeMap<String, ByteString>) -> Secret {
    Secret {
        metadata: ObjectMeta {
            name: Some(name.to_string()),
            ..Default::default()
        },
        data: Some(data),
        ..Default::default()
    }
}

pub fn create_secret_from_files(
    secret_name: &str,
    key_files: &[(PathBuf, &str)], //[pathbuf, key type]
) -> Result<Secret, Box<dyn Error>> {
    let mut data = BTreeMap::new();
    for (file_path, key_type) in key_files {
        let file_content = std::fs::read(file_path)
            .map_err(|err| format!("Failed to read file '{:?}': {}", file_path, err))?;
        data.insert(format!("{key_type}.json"), ByteString(file_content));
    }

    Ok(create_secret(secret_name, data))
}

pub fn create_selector(key: &str, value: &str) -> BTreeMap<String, String> {
    let mut btree = BTreeMap::new();
    btree.insert(key.to_string(), value.to_string());
    btree
}
