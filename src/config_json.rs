use serde_json::{Map, Value};
use std::fs::read;
use std::path::Path;

pub type ConfigMap = Map<String, Value>;
pub type ConfigResult = Result<String, String>;

pub struct ConfigJson {
    pub uuid: String,
    config: ConfigMap,
}

impl ConfigJson {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let json = read_config_json(path)?;

        Self::from_json(&json)
    }

    pub fn from_json(json: &str) -> Result<Self, String> {
        let config = json_object_from_string(json)?;
        let uuid = Self::get_config_value(&config, "uuid")?;
        Ok(Self { uuid, config })
    }

    fn get_config_value(config: &ConfigMap, key: &str) -> Result<String, String> {
        if let Some(value) = config.get(key) {
            if let Some(v) = value.as_str() {
                return Ok(v.to_string());
            }
        }

        Err(format!("key '{}' is bad or missing", key))
    }

    fn strip_api_endpoint(api_endpoint: &str) -> String {
        if api_endpoint.starts_with("https://") {
            api_endpoint[8..].into()
        } else if api_endpoint.starts_with("http://") {
            api_endpoint[7..].into()
        } else {
            api_endpoint.into()
        }
    }

    pub fn get_api_endpoint(&self) -> ConfigResult {
        Self::get_config_value(&self.config, "apiEndpoint")
    }

    pub fn get_api_root_certificate(&self) -> Option<String> {
        if let Ok(encoded) = Self::get_config_value(&self.config, "balenaRootCA") {
            if let Ok(bytes) = base64::decode(&encoded) {
                if let Ok(pem) = String::from_utf8(bytes) {
                    return Some(pem);
                }
            }
        }

        None
    }

    pub fn get_api_key_for_endpoint(&self, api_endpoint: &str) -> ConfigResult {
        if let Some(keys_value) = &self.config.get("deviceApiKeys") {
            if let Some(keys) = keys_value.as_object() {
                if let Some(value) = keys.get(&Self::strip_api_endpoint(api_endpoint)) {
                    if let Some(api_key) = value.as_str() {
                        return Ok(api_key.to_string());
                    }
                }
            }
        }

        Err(format!(
            "Unable to determine API key for endpoint {}",
            api_endpoint
        ))
    }
}

pub fn read_config_json(path: &Path) -> Result<String, String> {
    let contents =
        read(path).map_err(|why| format!("Unable to read file {}: {:?}", path.display(), why))?;

    String::from_utf8(contents).map_err(|why| format!("Unable to load JSON file: {:?}", why))
}

fn json_object_from_string(contents: &str) -> Result<ConfigMap, String> {
    let value: Value = serde_json::from_str(contents)
        .map_err(|why| format!("Unable to deserialize JSON: {:?}", why))?;

    if let Value::Object(map) = value {
        Ok(map)
    } else {
        Err("JSON did not represent a Map type".to_string())
    }
}
