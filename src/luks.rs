use crate::plugin::{volume, VolumeDriver};
use std::collections::HashMap;

pub struct LuksVolumeDriver {
    pub data_dir: String,
    pub mount_dir: String,
}

impl VolumeDriver for LuksVolumeDriver {
    fn create(&self, _name: String, _opts: Option<HashMap<String, String>>) -> Result<(), String> {
        Err("Not Implemented".to_string())
    }
    fn remove(&self, _name: String) -> Result<(), String> {
        Err("Not Implemented".to_string())
    }
    fn mount(&self, _name: String, _id: String) -> Result<String, String> {
        Err("Not Implemented".to_string())
    }
    fn path(&self, _name: String) -> Result<String, String> {
        Err("Not Implemented".to_string())
    }
    fn unmount(&self, _name: String, _id: String) -> Result<(), String> {
        Err("Not Implemented".to_string())
    }
    fn get(&self, _name: String) -> Result<volume::Volume, String> {
        Err("Not Implemented".to_string())
    }
    fn list(&self) -> Result<Vec<volume::Volume>, String> {
        Err("Not Implemented".to_string())
    }
}
