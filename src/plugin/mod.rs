pub mod rpc_request;
pub mod volume;

use crate::plugin::rpc_request::RpcRequest;
use actix_web::{middleware, web, App, HttpRequest, HttpResponse, HttpServer};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::sync::Arc;

use volume::{
    Capabilities, CreateVolumeRequest, GetVolumeRequest, MountVolumeRequest, PathVolumeRequest,
    RemoveVolumeRequest, Scope, Volume,
};

type RpcResponse = HttpResponse;

#[derive(Serialize, Deserialize, PartialEq)]
struct RpcError {
    #[serde(rename = "Err")]
    err: String,
}

impl Default for RpcError {
    fn default() -> Self {
        Self {
            err: String::default(),
        }
    }
}

impl RpcError {
    fn from_str(err: &str) -> Self {
        Self {
            err: String::from(err),
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    VolumeDriver,
}

#[derive(Serialize, Deserialize, PartialEq)]
pub struct ActivateResponse {
    #[serde(rename = "Implements")]
    pub implements: Vec<Protocol>,
}

impl Default for ActivateResponse {
    fn default() -> Self {
        Self {
            implements: Vec::new(),
        }
    }
}

pub trait VolumeDriver: Send + Sync {
    fn create(&self, name: String, opts: Option<HashMap<String, String>>) -> Result<(), String>;
    fn remove(&self, name: String) -> Result<(), String>;
    fn mount(&self, name: String, id: String) -> Result<String, String>;
    fn path(&self, name: String) -> Result<String, String>;
    fn unmount(&self, name: String, id: String) -> Result<(), String>;
    fn get(&self, name: String) -> Result<Volume, String>;
    fn list(&self) -> Result<Vec<Volume>, String>;
}

pub struct VolumePlugin<T> {
    socket_path: std::path::PathBuf,
    volume_driver: Arc<T>,
}

impl<T> VolumePlugin<T>
where
    T: VolumeDriver + 'static,
{
    pub fn new(socket: &std::path::Path, driver: Arc<T>) -> Self {
        Self {
            socket_path: socket.to_path_buf(),
            volume_driver: driver,
        }
    }

    pub fn start(self: &Self) -> io::Result<()> {
        if let Err(err) = fs::remove_file(&self.socket_path) {
            if err.kind() != io::ErrorKind::NotFound {
                return Err(err);
            }
        }
        info!(
            "Listening on unix://{path}",
            path = self.socket_path.to_str().unwrap()
        );

        let driver = Arc::clone(&self.volume_driver);
        let socket_path = self.socket_path.to_owned();
        HttpServer::new(move || {
            App::new()
                .data(driver.clone())
                .wrap(middleware::Logger::default())
                .service(
                    web::resource("/Plugin.Activate").route(
                        web::post().to(|| -> HttpResponse { Self::handle_plugin_activate() }),
                    ),
                )
                .service(web::resource("/VolumeDriver.Create").route(web::post().to(
                    move |create_request: RpcRequest<CreateVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_create(
                            create_request.0,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.Remove").route(web::post().to(
                    move |remove_request: RpcRequest<RemoveVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_remove(
                            remove_request.0.name,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.Mount").route(web::post().to(
                    move |mount_request: RpcRequest<MountVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_mount(
                            mount_request.0.name,
                            mount_request.0.id,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.Path").route(web::post().to(
                    move |path_request: RpcRequest<PathVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_path(
                            path_request.0.name,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.Unmount").route(web::post().to(
                    move |mount_request: RpcRequest<MountVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_unmount(
                            mount_request.0.name,
                            mount_request.0.id,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.Get").route(web::post().to(
                    move |get_request: RpcRequest<GetVolumeRequest>,
                          req: HttpRequest|
                          -> HttpResponse {
                        Self::handle_volume_get(
                            get_request.0.name,
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(web::resource("/VolumeDriver.List").route(web::post().to(
                    move |req: HttpRequest| -> HttpResponse {
                        Self::handle_volume_list(
                            req.app_data::<Arc<T>>().expect("No driver found").clone(),
                        )
                    },
                )))
                .service(
                    web::resource("/VolumeDriver.Capabilities").route(web::post().to(
                        move || -> HttpResponse {
                            HttpResponse::Ok().json(Capabilities {
                                scope: Scope::Local,
                            })
                        },
                    )),
                )
        })
        .bind_uds(socket_path)?
        .run()
    }

    fn handle_plugin_activate() -> RpcResponse {
        let plugin_implements = ActivateResponse {
            implements: vec![Protocol::VolumeDriver],
        };

        HttpResponse::Ok().json(plugin_implements)
    }

    fn handle_volume_create(
        create_request: volume::CreateVolumeRequest,
        driver: Arc<T>,
    ) -> RpcResponse {
        match T::create(&driver, create_request.name, create_request.opts) {
            Ok(_) => HttpResponse::Ok().json(RpcError::default()),
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }

    fn handle_volume_remove(name: String, driver: Arc<T>) -> RpcResponse {
        match T::remove(&driver, name) {
            Ok(_) => HttpResponse::Ok().json(RpcError::default()),
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }
    fn handle_volume_mount(name: String, id: String, driver: Arc<T>) -> RpcResponse {
        match T::mount(&driver, String::from(&name), id) {
            Ok(mountpoint) => {
                println!("{} {}", &name, mountpoint);
                HttpResponse::Ok().json(volume::MountVolumeResponse {
                    mountpoint,
                    err: "".to_string(),
                })
            }
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }
    fn handle_volume_path(name: String, driver: Arc<T>) -> RpcResponse {
        match T::path(&driver, name) {
            Ok(mountpoint) => {
                println!("{}", mountpoint);
                HttpResponse::Ok().json(volume::MountVolumeResponse {
                    mountpoint,
                    err: "".to_string(),
                })
            }
            Err(e) => {
                info!("{}", e);
                HttpResponse::BadRequest().json(RpcError::from_str(&e))
            }
        }
    }
    fn handle_volume_unmount(name: String, id: String, driver: Arc<T>) -> RpcResponse {
        match T::unmount(&driver, name, id) {
            Ok(_) => HttpResponse::Ok().json(RpcError::default()),
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }
    fn handle_volume_get(name: String, driver: Arc<T>) -> RpcResponse {
        println!("{:?}", name);
        match T::get(&driver, name) {
            Ok(vol) => {
                println!("{:?}", vol.mountpoint);
                HttpResponse::Ok().json(volume::GetVolumeResponse {
                    volume: volume::Volume {
                        name: vol.name,
                        mountpoint: vol.mountpoint,
                    },
                    err: "".to_string(),
                })
            }
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }
    fn handle_volume_list(driver: Arc<T>) -> RpcResponse {
        match T::list(&driver) {
            Ok(vols) => HttpResponse::Ok().json(volume::ListVolumesResponse {
                volumes: vols,
                err: "".to_string(),
            }),
            Err(e) => HttpResponse::BadRequest().json(RpcError::from_str(&e)),
        }
    }
}
