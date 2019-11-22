use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq)]
pub struct Volume {
    #[serde(rename = "Name")]
    pub name: String,

    #[serde(rename = "Mountpoint")]
    pub mountpoint: Option<String>,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct CreateVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Opts")]
    pub opts: Option<std::collections::HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct RemoveVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct MountVolumeRequest {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "ID")]
    pub id: String,
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
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Mountpoint")]
    pub mountpoint: Option<String>,
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
