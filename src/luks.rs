use crate::plugin::{volume, VolumeDriver};

use block_utils::{format_block_device, Filesystem};

use cryptsetup_rs::api::{CryptDeviceHandle, Luks1CryptDevice, Luks1Params};
use cryptsetup_rs::{crypt_rng_type, format, open};

use loopdev::{LoopControl, LoopDevice};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

pub struct LuksVolumeDriver {
    pub data_dir: String,
    pub mount_dir: String,
}

impl LuksVolumeDriver {
    fn create_disk_image(&self, location: &Path) -> Result<(), ()> {
        Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", location.to_str().unwrap()))
            .arg("bs=1")
            .arg("count=1")
            .arg("seek=1G")
            .status()
            .map(|_| ())
            .map_err(|_| ())
    }
    fn attach_loop_device(&self, image: &Path) -> Result<LoopDevice, ()> {
        let loop_control = LoopControl::open().unwrap();
        match loop_control.next_free() {
            Ok(device) => {
                device.attach_file(&image).unwrap();
                Ok(device)
            }
            Err(_) => Err(()),
        }
    }
    fn get_luks_device(&self, image: &Path) -> Result<CryptDeviceHandle<Luks1Params>, ()> {
        match self.attach_loop_device(image) {
            Ok(loop_dev) => match open(&loop_dev.path().unwrap()).unwrap().luks1() {
                Ok(existing_dev) => Ok(existing_dev),
                _ => Err(()),
            },
            _ => Err(()),
        }
    }
    fn format_luks_device(
        &self,
        image: &Path,
        key: &str,
    ) -> Result<CryptDeviceHandle<Luks1Params>, ()> {
        match self.attach_loop_device(image) {
            Ok(loop_dev) => {
                let uuid = Uuid::new_v4();
                let builder = format(&loop_dev.path().unwrap()).unwrap();
                match builder
                    .rng_type(crypt_rng_type::CRYPT_RNG_URANDOM)
                    .iteration_time(5000)
                    .luks1("aes", "xts-plain", "sha256", 256, Some(&uuid))
                {
                    Ok(mut device_handle) => {
                        let key = String::from(key).into_bytes();
                        match device_handle.add_keyslot(&key, None, None) {
                            Ok(_) => Ok(device_handle),
                            _ => Err(()),
                        }
                    }
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }
    fn open_luks_device(
        &self,
        name: &str,
        device: CryptDeviceHandle<Luks1Params>,
        key: &str,
    ) -> Result<(), ()> {
        let mut device = device;
        match device.activate(name, &String::from(key).into_bytes()) {
            Ok(_) => Ok(()),
            _ => Err(()),
        }
    }
    fn detach_dm_device(&self, name: &str) -> Result<(), String> {
        let dm =
            devicemapper::DM::new().map_err(|_| "Unable to create devicemapper".to_string())?;

        dm.device_remove(
            &devicemapper::DevId::Name(devicemapper::DmName::new(name).unwrap()),
            &devicemapper::DmOptions::new(),
        )
        .map(|_| ())
        .map_err(|why| format!("Unable to remove /dev/mapper/{}: {}", name, why))
    }
}

impl VolumeDriver for LuksVolumeDriver {
    fn create(&self, name: String, _opts: Option<HashMap<String, String>>) -> Result<(), String> {
        let volume_dir = Path::new(&self.data_dir).join(&name);
        if fs::create_dir_all(&volume_dir).is_err() {
            return Err(format!("Unable to create volume {}", &name));
        };

        let volume_img = &volume_dir.join("volume.img");
        self.create_disk_image(&volume_img)
            .map_err(|_| String::from("Couldn't create the empty disk image"))?;
        match self
            .format_luks_device(&volume_img, "secret key")
            .map_err(|_| String::from("Unable to format new LUKS volume header"))
        {
            Ok(device) => {
                let uuid = Uuid::new_v4();
                let path = format!("/dev/mapper/{}", &uuid.to_string());
                self.open_luks_device(&uuid.to_string(), device, "secret key")
                    .map(|_| String::default())
                    .map_err(|_| String::from("Unable to open the LUKS volume"))?;
                let xfs_options = Filesystem::Ext4 {
                    inode_size: 512,
                    stride: Some(2),
                    stripe_width: None,
                    reserved_blocks_percentage: 10,
                };
                format_block_device(Path::new(&path), &xfs_options)
                    .map(|_| ())
                    .map_err(|_| format!("Unable to format the LUKS device as Ext4: {}", &path))?;
                self.detach_dm_device(&uuid.to_string())
            }
            Err(e) => Err(e),
        }
    }
    fn remove(&self, name: String) -> Result<(), String> {
        let volume_dir = Path::new(&self.data_dir).join(&name);
        fs::remove_dir_all(&volume_dir).map_err(|why| {
            format!(
                "Unable to remove volume dir {}: {}",
                &volume_dir.to_str().unwrap(),
                why
            )
        })
    }
    fn mount(&self, name: String, id: String) -> Result<String, String> {
        let volume_img = Path::new(&self.data_dir).join(&name).join("volume.img");
        let mount_dir = Path::new(&self.mount_dir).join(&id);
        let _ = self.detach_dm_device(&id);
        fs::create_dir_all(&mount_dir).map(|_| ()).map_err(|why| {
            format!(
                "Unable to create mount dir {}: {}",
                mount_dir.to_str().unwrap(),
                why
            )
        })?;

        match self.get_luks_device(&volume_img) {
            Ok(device) => {
                self.open_luks_device(&id, device, "secret key")
                    .map(|_| String::default())
                    .map_err(|_| String::from("Unable to open the LUKS volume"))?;

                let supported = sys_mount::SupportedFilesystems::new()
                    .map_err(|why| format!("failed to get supported filesystems: {}", why))?;
                let src = Path::new("/dev/mapper").join(&id);

                sys_mount::Mount::new(
                    &src,
                    &mount_dir,
                    &supported,
                    sys_mount::MountFlags::empty(),
                    None,
                )
                .map_err(|why| {
                    format!(
                        "failed to get mount {} to {}: {}",
                        &src.to_str().unwrap(),
                        &mount_dir.to_str().unwrap(),
                        why
                    )
                })
                .map(|_| String::from(mount_dir.to_str().unwrap()))
            }
            _ => Err(String::from("Unable to get the LUKS device")),
        }
    }
    fn path(&self, _name: String) -> Result<String, String> {
        Err("Not Implemented".to_string())
    }
    fn unmount(&self, _name: String, id: String) -> Result<(), String> {
        let mnt_dir = Path::new(&self.mount_dir).join(&id);
        sys_mount::unmount(&mnt_dir, sys_mount::UnmountFlags::empty())
            .map_err(|why| format!("failed to umount {}: {}", &mnt_dir.to_str().unwrap(), why))
            .map(|_| ())?;
        fs::remove_dir_all(&mnt_dir).map_err(|why| {
            format!(
                "Unable to remove mount dir {}: {}",
                &mnt_dir.to_str().unwrap(),
                why
            )
        })?;
        self.detach_dm_device(&id).map(|_| ())
    }
    fn get(&self, name: String) -> Result<volume::Volume, String> {
        match fs::metadata(Path::new(&self.data_dir).join(&name).join("volume.img")) {
            Ok(_) => Ok(volume::Volume {
                mountpoint: Some(String::from("")),
                name,
            }),
            _ => Err(format!("Unable to find volume {}", name)),
        }
    }
    fn list(&self) -> Result<Vec<volume::Volume>, String> {
        let volumes: Vec<volume::Volume> = fs::read_dir(Path::new(&self.data_dir))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|f| f.metadata().unwrap().is_dir())
            .map(|f| volume::Volume {
                name: String::from(f.path().file_name().unwrap().to_str().unwrap()),
                mountpoint: Some(String::from("")),
            })
            .collect();

        Ok(volumes)
    }
}
