pub mod volume;

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fs;
use std::io;

use hyper::service::service_fn;
use hyper::{Body, Method, Request, Response, StatusCode};

use futures::stream::Stream;
use futures::Future;

use std::sync::Arc;

use volume::Volume;

type RouterResponse =
    Box<dyn Future<Item = Response<Body>, Error = Box<dyn std::error::Error + Send + Sync>> + Send>;

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
    __socket: std::path::PathBuf,
    __driver: Arc<T>,
}

impl<T> VolumePlugin<T>
where
    T: VolumeDriver + 'static,
{
    pub fn new(socket: &std::path::Path, driver: Arc<T>) -> Self {
        Self {
            __socket: socket.to_path_buf(),
            __driver: driver,
        }
    }

    pub fn start(self: &Self) -> io::Result<()> {
        if let Err(err) = fs::remove_file(&self.__socket) {
            if err.kind() != io::ErrorKind::NotFound {
                return Err(err);
            }
        }
        println!(
            "Listening on unix://{path} with 1 thread.",
            path = self.__socket.to_str().unwrap()
        );

        let driver = Arc::clone(&self.__driver);
        let svr = hyperlocal::server::Server::bind(&self.__socket, move || {
            let inner = Arc::clone(&driver);
            service_fn(move |req| Self::router(req, &inner))
        })
        .expect("unable to create the socket server");
        svr.run().expect("unable to start the socket server");
        Ok(())
    }

    fn router(req: Request<Body>, driver: &Arc<T>) -> RouterResponse {
        let (parts, body) = req.into_parts();
        let driver = Arc::clone(&driver);
        Box::new(body.concat2().from_err().and_then(move |body| {
            let working_driver = Arc::clone(&driver);
            let payload = match String::from_utf8(body.to_vec()) {
                Ok(v) => v,
                Err(_) => "".to_string(),
            };

            println!("-> {} {}", parts.method, parts.uri.path());

            let (status, response) = match (parts.method, parts.uri.path()) {
                (Method::POST, "/Plugin.Activate") => {
                    Self::handle_plugin_activate(ActivateResponse {
                        implements: vec![Protocol::VolumeDriver],
                    })
                }
                (Method::POST, "/VolumeDriver.Create") => {
                    match serde_json::from_str::<volume::CreateVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_create(p, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.Remove") => {
                    match serde_json::from_str::<volume::RemoveVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_remove(p.name, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.Mount") => {
                    match serde_json::from_str::<volume::MountVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_mount(p.name, p.id, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.Path") => {
                    match serde_json::from_str::<volume::PathVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_path(p.name, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.Unmount") => {
                    match serde_json::from_str::<volume::MountVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_unmount(p.name, p.id, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.Get") => {
                    match serde_json::from_str::<volume::GetVolumeRequest>(&payload) {
                        Ok(p) => Self::handle_volume_get(p.name, working_driver),
                        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()),
                    }
                }
                (Method::POST, "/VolumeDriver.List") => Self::handle_volume_list(working_driver),
                _ => (StatusCode::BAD_REQUEST, String::from("Not Implemented")),
            };

            println!("<- {} {}", &status, &response);

            Ok(Response::builder()
                .status(status)
                .body(response.into())
                .unwrap())
        }))
    }

    fn handle_plugin_activate(response: ActivateResponse) -> (StatusCode, String) {
        (
            StatusCode::OK,
            serde_json::to_string(&response).unwrap_or_default(),
        )
    }

    fn handle_volume_create(
        create_request: volume::CreateVolumeRequest,
        driver: Arc<T>,
    ) -> (StatusCode, String) {
        match T::create(&driver, create_request.name, create_request.opts) {
            Ok(_) => (StatusCode::OK, String::from("{ \"Err\": \"\" }")),
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }

    fn handle_volume_remove(name: String, driver: Arc<T>) -> (StatusCode, String) {
        match T::remove(&driver, name) {
            Ok(_) => (StatusCode::OK, String::from("{ \"Err\": \"\" }")),
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
    fn handle_volume_mount(name: String, id: String, driver: Arc<T>) -> (StatusCode, String) {
        match T::mount(&driver, name, id) {
            Ok(mountpoint) => {
                match serde_json::to_string(&volume::MountVolumeResponse {
                    mountpoint,
                    err: "".to_string(),
                }) {
                    Ok(p) => (StatusCode::OK, p),
                    Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
                }
            }
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
    fn handle_volume_path(name: String, driver: Arc<T>) -> (StatusCode, String) {
        match T::path(&driver, name) {
            Ok(mountpoint) => {
                match serde_json::to_string(&volume::MountVolumeResponse {
                    mountpoint,
                    err: "".to_string(),
                }) {
                    Ok(p) => (StatusCode::OK, p),
                    Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
                }
            }
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
    fn handle_volume_unmount(name: String, id: String, driver: Arc<T>) -> (StatusCode, String) {
        match T::unmount(&driver, name, id) {
            Ok(_) => (StatusCode::OK, String::from("{ \"Err\": \"\" }")),
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
    fn handle_volume_get(name: String, driver: Arc<T>) -> (StatusCode, String) {
        match T::get(&driver, name) {
            Ok(vol) => {
                match serde_json::to_string(&volume::GetVolumeResponse {
                    volume: volume::Volume {
                        name: vol.name,
                        mountpoint: vol.mountpoint,
                    },
                    err: "".to_string(),
                }) {
                    Ok(p) => (StatusCode::OK, p),
                    Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
                }
            }
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
    fn handle_volume_list(driver: Arc<T>) -> (StatusCode, String) {
        match T::list(&driver) {
            Ok(vols) => {
                match serde_json::to_string(&volume::ListVolumesResponse {
                    volumes: vols,
                    err: "".to_string(),
                }) {
                    Ok(p) => (StatusCode::OK, p),
                    Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
                }
            }
            Err(e) => (StatusCode::BAD_REQUEST, format!(r#"{{ "Err": "{}" }}"#, e)),
        }
    }
}
