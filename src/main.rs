extern crate byteorder;
extern crate num_traits;
extern crate quickxml_to_serde;
#[macro_use]
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate serde;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate config;
extern crate fern;

extern crate crossbeam_channel as channel;
extern crate crossbeam;
extern crate rand;

extern crate actix_web;

mod settings;
mod xml_to_struct;
mod networking;

use std::collections::HashMap;
use std::path::Path;

use actix_web::{server, App, HttpRequest, Responder};

fn file_exists<T: AsRef<Path>>(path: T) -> bool {
    path.as_ref().exists() && path.as_ref().is_file()
}

fn main() {
    let matches: clap::ArgMatches = clap_app!(adsserver =>
        (version: "1.0")
        (author: "Lukas Binder")
        (about: "rust ads server")
        (@arg CONFIG: -c #{1,2} "Sets a custom config file")
        (@arg INPUT: "Sets the input directory to use")
        (@arg debug: -v ... "Sets the level of debugging information")
    ).get_matches();
    let log_level = match matches.occurrences_of("debug") {
        0 => (log::LevelFilter::Error, log::LevelFilter::Warn),
        1 => (log::LevelFilter::Info, log::LevelFilter::Debug),
        2 | _ => (log::LevelFilter::Trace, log::LevelFilter::Trace),
    };
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%d.%m.%Y/%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .chain(
            fern::Dispatch::new()
                .level(log_level.0)
                .chain(std::io::stdout()),
        )
        .chain(
            fern::Dispatch::new()
                .level(log_level.1)
                .chain(fern::log_file("adsserver.log").unwrap()),
        )
        .apply()
        .unwrap();

    let mut settings = config::Config::default();
    let config_file = matches.value_of("CONFIG").unwrap_or("config.json");
    settings
        .merge(config::File::with_name(config_file))
        .unwrap()
        .merge(config::Environment::with_prefix("APP"))
        .unwrap();
    let config = settings.try_into::<settings::Setting>().unwrap();
    let t: HashMap<u32, _> = config
        .versions
        .iter()
        .filter_map(|version| {
            let p: &Path = version.1.path.as_ref();
            if file_exists(&p) {
                let u: u32 = version.0.into();
                Some((u, xml_to_struct::read_tpy(version.1)))
            } else {
                error!("version file {:?} does not exist", p);
                None
            }
        })
        .collect();
    ws::listen("0.0.0.0:8000", |out| {
        move |msg| {
            eprintln!("msg = {:#?}", msg);
            out.send(msg)
        }
    }).unwrap();
}
