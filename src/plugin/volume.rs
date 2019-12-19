use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq)]
pub enum Scope {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "global")]
    Global,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    #[serde(rename = "Scope")]
    pub scope: Scope,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Volume {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Mountpoint")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mountpoint: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct CreateVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Opts")]
    pub opts: Option<std::collections::HashMap<String, String>>,
}

impl Default for CreateVolumeRequest {
    fn default() -> Self {
        Self {
            name: String::default(),
            opts: None,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct RemoveVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
}

impl Default for RemoveVolumeRequest {
    fn default() -> Self {
        Self {
            name: String::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct MountVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "ID")]
    pub id: String,
}

impl Default for MountVolumeRequest {
    fn default() -> Self {
        Self {
            name: String::default(),
            id: String::default(),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct MountVolumeResponse {
    #[serde(rename = "Mountpoint")]
    pub mountpoint: String,
    #[serde(rename = "Err")]
    pub err: String,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct GetVolumeResponse {
    #[serde(rename = "Volume")]
    pub volume: Volume,
    #[serde(rename = "Err")]
    pub err: String,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct ListVolumesResponse {
    #[serde(rename = "Volumes")]
    pub volumes: Vec<Volume>,
    #[serde(rename = "Err")]
    pub err: String,
}

pub type PathVolumeRequest = RemoveVolumeRequest;
pub type GetVolumeRequest = RemoveVolumeRequest;
