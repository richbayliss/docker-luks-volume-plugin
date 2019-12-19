use crate::crypto::{DummyHSM, VirtualHSM};
use crate::plugin::{volume, VolumeDriver};

use block_utils::{format_block_device, Filesystem};

use cryptsetup_rs::api::{CryptDeviceHandle, Luks1CryptDevice, Luks1Params};
use cryptsetup_rs::{crypt_rng_type, format, open};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

pub type DriverHSM = dyn VirtualHSM + Send + Sync;

pub struct LuksVolumeDriver {
    pub data_dir: PathBuf,
    pub mount_dir: PathBuf,
    hsm: Box<DriverHSM>,
}

impl LuksVolumeDriver {
    pub fn new(data_dir: &str, mount_dir: &str, hsm: Option<Box<DriverHSM>>) -> Self {
        Self {
            data_dir: Path::new(data_dir)
                .canonicalize()
                .expect("Not a valid path for data_dir"),
            mount_dir: Path::new(mount_dir)
                .canonicalize()
                .expect("Not a valid path for data_dir"),
            hsm: match hsm {
                Some(hsm) => hsm,
                None => Box::new(DummyHSM::new()),
            },
        }
    }

    fn get_luks_key(&self, name: &str) -> Result<Vec<u8>, String> {
        let key_file = &self.data_dir.join(&name).join("keyfile");
        fs::metadata(&key_file)
            .map(|_| &key_file)
            .map_err(|why| format!("Unable to get key for volume {}: {:?}", &name, why))?;

        let key_data = fs::read(&key_file)
            .map_err(|why| format!("Unable to read key file {}: {:?}", &key_file.display(), why))?;
        self.hsm
            .decrypt(key_data)
            .map_err(|e| format!("Unable to decrypt key file {}: {}", &key_file.display(), e))
    }

    fn store_luks_key(&self, name: &str, key_data: Vec<u8>) -> Result<(), String> {
        let key_file = &self.data_dir.join(&name).join("keyfile");

        let encrypted_blob = self
            .hsm
            .encrypt(key_data.to_vec())
            .map_err(|e| format!("Unable to encrypt key {}: {}", &key_file.display(), e))?;

        fs::write(&key_file, &encrypted_blob)
            .map_err(|why| format!("Unable to wite key file {}: {:?}", &key_file.display(), why))?;

        Ok(())
    }

    fn create_disk_image(&self, location: &Path) -> Result<(), String> {
        Command::new("dd")
            .arg("if=/dev/zero")
            .arg(format!("of={}", location.to_str().unwrap()))
            .arg("bs=1G")
            .arg("count=0")
            .arg("seek=1")
            .status()
            .map(|_| ())
            .map_err(|why| format!("Unable to create the disk image: {}", why))
    }
    fn format_luks_device(
        &self,
        image: &Path,
        key: &[u8],
    ) -> Result<CryptDeviceHandle<Luks1Params>, String> {
        let do_steps = || -> Result<CryptDeviceHandle<Luks1Params>, String> {
            let uuid = Uuid::new_v4();
            let builder = format(&image)
                .map_err(|_| "Unable to create a builder to format the LUKS image".to_string())?;
            let device_handle = builder
                .rng_type(crypt_rng_type::CRYPT_RNG_URANDOM)
                .iteration_time(5000)
                .luks1("aes", "xts-plain", "sha256", 256, Some(&uuid))
                .map_err(|_| "Unable to format the LUKS image".to_string())?;
            let mut device_handle = device_handle;
            device_handle
                .add_keyslot(&key, None, None)
                .map_err(|_| "Unable to add key to LUKS keyslot".to_string())?;
            Ok(device_handle)
        };

        match do_steps() {
            Ok(device_handle) => Ok(device_handle),
            Err(why) => {
                // cleanup files on the filesystem...
                // ...
                Err(format!("Unable to format the LUKS device: {}", why))
            }
        }
    }
    fn activate_luks_device(
        &self,
        name: &str,
        image: &Path,
        key: &[u8],
    ) -> Result<PathBuf, String> {
        let image = String::from(image.to_str().unwrap_or_default());
        let do_steps = || -> Result<PathBuf, String> {
            let mut device = open(&image)
                .map_err(|why| format!("Unable to open LUKS image {}: {:?}", &image, why))?
                .luks1()
                .map_err(|why| {
                    format!(
                        "Unable to get device handle for LUKS image {}: {:?}",
                        &image, why
                    )
                })?;

            device
                .activate(&name, &key)
                .map_err(|_| "Unable to activate LUKS device".to_string())?;

            Ok(PathBuf::from(format!("/dev/mapper/{}", &name)))
        };

        do_steps()
    }
    fn deactivate_luks_device(&self, name: &str, image: &Path) -> Result<(), String> {
        let image = String::from(image.to_str().unwrap_or_default());

        let device = open(&image)
            .map_err(|why| format!("Unable to open LUKS image {}: {:?}", &image, why))?
            .luks1()
            .map_err(|why| {
                format!(
                    "Unable to get device handle for LUKS image {}: {:?}",
                    &image, why
                )
            })?;
        device
            .deactivate(name)
            .map_err(|_| "Unable to deactivate LUKS device".to_string())
            .map(|_| ())?;

        Ok(())
    }
}

impl VolumeDriver for LuksVolumeDriver {
    fn create(&self, name: String, _opts: Option<HashMap<String, String>>) -> Result<(), String> {
        let volume_dir = &self.data_dir.join(&name);
        let volume_img = &volume_dir.join("volume.img");
        let secret_key = &self
            .hsm
            .random_bytes()
            .map_err(|e| format!("Unable to generate random bytes for new LUKS key: {}", e))?;

        let do_steps = || -> Result<(), String> {
            fs::create_dir_all(&volume_dir).map_err(|why| {
                format!(
                    "Unable to create the volume directory {}: {}",
                    &volume_dir.to_str().unwrap(),
                    why
                )
            })?;

            self.create_disk_image(&volume_img).map_err(|why| {
                format!(
                    "Couldn't create the LUKS disk image for the volume {}: {}",
                    name, why
                )
            })?;

            self.format_luks_device(&volume_img, &secret_key)
                .map_err(|why| {
                    format!("Unable to format LUKS header on the disk image: {}", why)
                })?;

            let uuid = Uuid::new_v4().to_string();
            let path = self
                .activate_luks_device(&uuid, &volume_img, &secret_key)
                .map_err(|why| format!("Unable to activate the LUKS disk image: {}", why))?;
            let path = String::from(path.to_str().unwrap());
            let xfs_options = Filesystem::Ext4 {
                inode_size: 256,
                reserved_blocks_percentage: 5,
                stride: None,
                stripe_width: None,
            };
            format_block_device(Path::new(&path), &xfs_options).map_err(|why| {
                format!(
                    "Unable to format the LUKS disk image {} as Ext4: {:?}",
                    &path, why
                )
            })?;

            self.deactivate_luks_device(&uuid, &volume_img)
                .map_err(|why| format!("Unable to deactive the LUKS disk image: {}", why))?;

            Ok(())
        };

        if let Err(why) = do_steps() {
            fs::remove_dir_all(&volume_dir).map_err(|why| {
                format!(
                    "Unable to remove the volume directory for \"{}\": {}",
                    name, why
                )
            })?;
            return Err(format!("Unable to create volume {}: {}", name, why));
        }

        self.store_luks_key(&name, secret_key.to_owned())?;

        Ok(())
    }
    fn remove(&self, name: String) -> Result<(), String> {
        let volume_dir = &self.data_dir.join(&name);
        fs::remove_dir_all(&volume_dir).map_err(|why| {
            format!(
                "Unable to remove volume dir {}: {}",
                &volume_dir.to_str().unwrap(),
                why
            )
        })
    }
    fn mount(&self, name: String, id: String) -> Result<String, String> {
        let volume_img = &self.data_dir.join(&name).join("volume.img");
        let mount_dir = &self.mount_dir.join(&name);
        let secret_key = &self.get_luks_key(&name)?;

        let do_steps = || -> Result<String, String> {
            fs::create_dir_all(&mount_dir).map(|_| ()).map_err(|why| {
                format!(
                    "Unable to create mount dir {}: {}",
                    mount_dir.to_str().unwrap(),
                    why
                )
            })?;

            let src = self
                .activate_luks_device(&id, &volume_img, &secret_key)
                .map(|p| String::from(p.to_str().unwrap()))
                .map_err(|_| String::from("Unable to open the LUKS volume"))?;

            let supported = sys_mount::SupportedFilesystems::new()
                .map_err(|why| format!("failed to get supported filesystems: {}", why))?;

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
                    &src,
                    &mount_dir.to_str().unwrap(),
                    why
                )
            })
            .map(|_| String::from(mount_dir.to_str().unwrap()))
        };

        match do_steps() {
            Ok(mountpoint) => Ok(mountpoint),
            Err(why) => {
                // tidy up ...
                Err(format!("Unable to mount the volume {}: {}", name, why))
            }
        }
    }
    fn path(&self, name: String) -> Result<String, String> {
        let mountpoint = self.mount_dir.join(&name).to_str().unwrap().to_owned();
        fs::metadata(&mountpoint)
            .map(|_| mountpoint)
            .map_err(|why| format!("Unable to get path for volume {}: {:?}", &name, why))
    }
    fn unmount(&self, name: String, id: String) -> Result<(), String> {
        let mnt_dir = &self.mount_dir.join(&name);
        let volume_img = &self.data_dir.join(&name).join("volume.img");
        let do_steps = || -> Result<(), String> {
            sys_mount::unmount(&mnt_dir, sys_mount::UnmountFlags::FORCE)
                .map_err(|why| format!("Failed to unmount {}: {}", &mnt_dir.to_str().unwrap(), why))
                .map(|_| ())?;
            self.deactivate_luks_device(&id, &volume_img)?;
            fs::remove_dir_all(&mnt_dir).map_err(|why| {
                format!(
                    "Unable to remove mount dir {}: {}",
                    &mnt_dir.to_str().unwrap(),
                    why
                )
            })?;
            Ok(())
        };

        do_steps().map_err(|why| format!("Unable to unmount {}: {}", name, why))
    }
    fn get(&self, name: String) -> Result<volume::Volume, String> {
        let do_steps = || -> Result<volume::Volume, String> {
            let _metadata = fs::metadata(&self.data_dir.join(&name).join("volume.img"))
                .map_err(|why| format!("Unable to find volume image: {}", why))?;
            let mountpoint = self.mount_dir.join(&name).to_str().unwrap().to_owned();
            let mountpoint = match fs::metadata(&mountpoint).map(|_| mountpoint) {
                Ok(m) => Some(m),
                Err(_) => None,
            };

            Ok(volume::Volume { mountpoint, name })
        };

        do_steps().map_err(|why| format!("Unable to get volume info: {}", why))
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
