mod luks;
mod plugin;

use clap::{App, Arg};
use std::path::Path;
use std::sync::Arc;

fn main() {
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
        .get_matches();

    let driver = luks::LuksVolumeDriver {
        data_dir: args
            .value_of("data_dir")
            .expect("A value for the --data-dir must be provided")
            .to_string(),
        mount_dir: args
            .value_of("mount_dir")
            .expect("A value for the --mount-dir must be provided")
            .to_string(),
    };

    let listen_socket = args
        .value_of("unix_socket")
        .expect("A value for --unix-socket must be provided");

    let host: plugin::VolumePlugin<luks::LuksVolumeDriver> =
        plugin::VolumePlugin::new(Path::new(&listen_socket), Arc::new(driver));

    if let Err(err) = host.start() {
        eprintln!("error starting plugin host: {}", err)
    }
}
