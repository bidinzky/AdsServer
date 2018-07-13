extern crate byteorder;
extern crate num_traits;
extern crate quickxml_to_serde;
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

extern crate crossbeam;
extern crate crossbeam_channel as channel;
extern crate rand;

extern crate actix;
extern crate actix_web;

extern crate chashmap;

extern crate bus;

mod networking;
mod settings;
mod ws;
mod xml_to_struct;

use std::path::Path;
use actix_web::{server, App, HttpRequest, Responder};
use networking::SimpleSocket;

fn file_exists<T: AsRef<Path>>(path: T) -> bool {
    path.as_ref().exists() && path.as_ref().is_file()
}

fn index(info: HttpRequest<ws::WsState>) -> impl Responder {
    serde_json::to_string_pretty(&*info.state().config.read().unwrap())
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
    let t: chashmap::CHashMap<u32, _> = config
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
    let v: Vec<_> = config
        .plc
        .iter()
        .map(|plc| {
            SimpleSocket::new(&plc.ip, (plc.ams_net_id.clone(), plc.ams_port), &config.connection_parameter)
        })
        .collect();
    println!("{:?}", v);
    let ws_state = ws::WsState {
        map: std::sync::Arc::new(t),
        config: std::sync::Arc::new(std::sync::RwLock::new(config.plc)),
    };
    server::new(move || {
        App::with_state(ws_state.clone())
            .middleware(actix_web::middleware::Logger::default())
            .resource("/ws/{net_id}/{port}/", |r| r.with(ws::Ws::ws_index))
            .resource("/", |r| r.with(index))
    }).bind("127.0.0.1:8000")
        .unwrap()
        .run();
}
