extern crate actix_http;
extern crate actix_web;
extern crate base64;
extern crate block_utils;
extern crate clap;
extern crate cryptsetup_rs;
extern crate derive_more;
extern crate futures;
extern crate lazy_static;
extern crate log;
extern crate openssl;
extern crate serde;
extern crate serde_json;
extern crate simple_logger;
extern crate url;
extern crate uuid;

mod config_json;
mod crypto;
mod hsm;
mod luks;
mod plugin;

use clap::{App, Arg};
use config_json::ConfigJson;
use std::path::Path;
use std::sync::Arc;

fn main() {
    simple_logger::init_with_level(log::Level::Info).expect("Unable to initialise the logger");

    let args = App::new("LUKS Volume Driver")
        .version("0.1")
        .author("Rich B. <richbayliss@gmail.com>")
        .about("Provides a Docker volume plugin for LUKS encrypted volumes.")
        .arg(
            Arg::with_name("unix_socket")
                .short("s")
                .long("unix-socket")
                .value_name("FILE")
                .help("The unix socket location to listen on.")
                .default_value("/run/docker/plugins/luks.sock")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("data_dir")
                .short("d")
                .long("data-dir")
                .value_name("DIR")
                .help("The directory to store LUKS encrypted volumes.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("mount_dir")
                .short("m")
                .long("mount-dir")
                .value_name("DIR")
                .help("The root directory to mount LUKS encrypted volumes into.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("config_json")
                .short("c")
                .long("config-json")
                .value_name("PATH")
                .help("The path to the config.json for this device.")
                .default_value("/mnt/boot/config.json")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("api_version")
                .short("V")
                .long("api-version")
                .env("CLOUDLOCK_API_VERSION")
                .value_name("VERSION")
                .help("The API version to use.")
                .default_value("v1")
                .takes_value(true),
        )
        .get_matches();

    let config_json_path = &args
        .value_of("config_json")
        .expect("A value for --config-json must be provided")
        .to_string();
    let api_version = &args
        .value_of("api_version")
        .expect("A value for --api-version must be provided")
        .to_string();

    let config =
        ConfigJson::from_file(Path::new(&config_json_path)).expect("Unable to read config.json");

    let hsm = hsm::cloudlock::CloudLockHSM::from_config(&config, api_version)
        .expect("Unable to initialise the CloudLock HSM");

    let driver = luks::LuksVolumeDriver::new(
        &args
            .value_of("data_dir")
            .expect("A value for the --data-dir must be provided")
            .to_string(),
        &args
            .value_of("mount_dir")
            .expect("A value for the --mount-dir must be provided")
            .to_string(),
        Some(Box::new(hsm)),
    );

    let listen_socket = args
        .value_of("unix_socket")
        .expect("A value for --unix-socket must be provided");

    let host: plugin::VolumePlugin<luks::LuksVolumeDriver> =
        plugin::VolumePlugin::new(Path::new(&listen_socket), Arc::new(driver));

    if let Err(err) = host.start() {
        eprintln!("error starting plugin host: {}", err)
    }
}
