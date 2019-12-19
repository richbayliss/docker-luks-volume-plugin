extern crate lazy_static;
extern crate simple_logger;

mod crypto;
mod hsm;
mod luks;
mod plugin;

use clap::{App, Arg};
use std::path::Path;
use std::sync::Arc;

fn main() {
    simple_logger::init_with_level(log::Level::Info).expect("Unable to initialise the logger");

    let args = App::new("LUKS Volume Driver")
        .version("1.0")
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
            Arg::with_name("device_uuid")
                .short("u")
                .long("device-uuid")
                .env("CLOUDLOCK_DEVICE_UUID")
                .value_name("UUID")
                .help("The UUID of the device.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("api_key")
                .short("k")
                .long("api-key")
                .env("CLOUDLOCK_API_KEY")
                .value_name("KEY")
                .help("The API key to use.")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("api_host")
                .short("h")
                .long("api-host")
                .env("CLOUDLOCK_API_HOST")
                .value_name("HOST")
                .help("The API host to use.")
                .default_value("api.balena-cloud.com")
                .required(false)
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

    let uuid = &args
        .value_of("device_uuid")
        .expect("A value for the --device-uuid must be provided")
        .to_string();
    let api_key = &args
        .value_of("api_key")
        .expect("A value for the --api-key must be provided")
        .to_string();
    let api_host = &args
        .value_of("api_host")
        .expect("A value for the --api-host must be provided")
        .to_string();
    let api_version = &args
        .value_of("api_version")
        .expect("A value for the --api-version must be provided")
        .to_string();

    let hsm = hsm::cloudlock::CloudLockHSM::new(uuid, api_key, api_host, api_version)
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
