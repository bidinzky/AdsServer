#![feature(nll)]
#![cfg_attr(feature = "cargo-clippy", allow(print_literal))]

extern crate actix;
extern crate byteorder;
extern crate bytes;
extern crate futures;
extern crate rand;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate tokio;
extern crate tokio_codec;
extern crate tokio_io;
extern crate tokio_tcp;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate log;
extern crate chrono;
extern crate config;
extern crate fern;
#[macro_use]
extern crate nom;
extern crate actix_web;
extern crate chashmap;
extern crate num_traits;
extern crate quickxml_to_serde;

mod json_diff;
mod networking;
mod settings;
mod types;
mod ws;
mod ws_ads;
mod xml_to_struct;

use actix::Actor;
use actix_web::{server, App, HttpRequest, Responder};
use futures::future::Future;
use networking::ToPlcConn;
use std::path::Path;
use std::sync::{Arc, RwLock};
use ws_ads::AdsStructMap;

#[inline(always)]
fn file_exists<T: AsRef<Path>>(path: T) -> bool {
    path.as_ref().exists() && path.as_ref().is_file()
}

#[inline(always)]
#[cfg_attr(feature = "cargo-clippy", allow(needless_pass_by_value))]
fn index(info: HttpRequest<Arc<ws::WsState>>) -> impl Responder {
    serde_json::to_string_pretty(&*info.state().config())
}

fn init_st_ads_to_bc<T: ToString + serde_json::value::Index>(
    version: &types::AdsVersion,
    k: &T,
    d: &mut serde_json::Value,
) -> Vec<u8> {
    let key_guard: &String = &*version.search_index.get(&k.to_string()).unwrap();
    let value: &types::AdsType = &*version.map.get(&*key_guard).unwrap();
    let v = vec![0u8; value.len() as usize];
    let data = value.as_data_struct(&mut &v[..], &version.map);
    d[k] = data;
    v
}

fn main() {
    let system = actix::System::new("adsserver");

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
    let sps_types: chashmap::CHashMap<u32, _> = config
        .versions
        .iter()
        .filter_map(|version| {
            let p: &Path = version.1.path.as_ref();
            if file_exists(&p) {
                let u: u32 = version.0.into();
                Some((u, Arc::new(xml_to_struct::read_tpy(version.1))))
            } else {
                error!("version file {:?} does not exist", p);
                None
            }
        })
        .collect();
    let sender: chashmap::CHashMap<_, _> = config
        .plc
        .iter()
        .map(move |plc| {
            let version = &*sps_types.get(&plc.version).expect("unknown version");
            let mkey = version
                .search_index
                .get(&"ST_ADS_TO_BC".to_string())
                .unwrap();
            let rkey = version
                .search_index
                .get(&"ST_RETAIN_DATA".to_string())
                .unwrap();

            let m = AdsStructMap {
                st_ads_to_bc: version.symbols.get(&*mkey).unwrap().clone(),
                st_retain_data: version.symbols.get(&*rkey).unwrap().clone(),
            };
            let mut data = serde_json::Value::Object(serde_json::Map::new());
            let mem = ws_ads::AdsMemory {
                ST_ADS_TO_BC: init_st_ads_to_bc(&version, &"ST_ADS_TO_BC", &mut data),
                ST_ADS_FROM_BC: init_st_ads_to_bc(&version, &"ST_ADS_FROM_BC", &mut data),
                ST_RETAIN_DATA: init_st_ads_to_bc(&version, &"ST_RETAIN_DATA", &mut data),
                data,
            };
            let conn = (plc.ams_net_id.clone(), plc.ams_port).as_plc_conn();
            let client_addr = networking::create_client(
                (plc.ip.as_str(), 48898),
                &conn,
                &("172.16.21.2.1.1", 801),
            ).wait()
                .unwrap();
            (
                conn,
                ws_ads::AdsToWsMultiplexer::new(client_addr, mem, version.clone(), m).start(),
            )
        })
        .collect();
    let ws_state = Arc::new(ws::WsState::new(RwLock::new(config.plc), sender));

    server::new(move || {
        App::with_state(ws_state.clone())
            .middleware(actix_web::middleware::Logger::default())
            .resource("/ws/{net_id}/{port}/", |r| r.with(ws::Ws::ws_index))
            .resource("/", |r| r.with(index))
    }).bind("127.0.0.1:8000")
        .unwrap()
        .start();

    let _ = system.run();
}
