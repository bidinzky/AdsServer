use super::settings::PlcSetting;
use super::xml_to_struct::types::{AdsType, AdsVersion};
use actix::prelude::*;
use actix_web::{ws, Error, HttpRequest, HttpResponse};
use chashmap::CHashMap;
use json_diff::{merge, schema_parser, Schema};
use networking::ToPlcConn;
use serde_json::{self, to_string, Value};
use std::io;
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct AdsMemory {
    ST_ADS_TO_BC: Vec<u8>,
    ST_ADS_FROM_BC: Vec<u8>,
    ST_RETAIN_DATA: Vec<u8>,
}

impl AdsMemory {
    pub fn by_str(&mut self, s: &str) -> &mut Vec<u8> {
        match s.trim() {
            "ST_ADS_TO_BC" => &mut self.ST_ADS_TO_BC,
            "ST_ADS_FROM_BC" => &mut self.ST_ADS_FROM_BC,
            "ST_RETAIN_DATA" => &mut self.ST_RETAIN_DATA,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug)]
pub struct WsState {
    pub map: CHashMap<u32, AdsVersion>,
    pub config: RwLock<Vec<PlcSetting>>,
    pub data: CHashMap<[u8; 8], AdsMemory>,
}

#[derive(Debug)]
pub struct Ws {
    plc_conn: [u8; 8],
    version: u32,
}

impl Ws {
    pub fn ws_index(r: HttpRequest<Arc<WsState>>) -> Result<HttpResponse, Error> {
        let (version, plc_conn) = ({
            let m = r.match_info();
            let net_id = m.query::<String>("net_id")?;
            let port = m.query::<u16>("port")?;
            let lg = r.state().config.read().unwrap();
            let mys = lg.iter().find(|x| x.ams_net_id == net_id);
            match mys {
                Some(c) => Ok((c.version, (net_id, port).try_into_plc_conn().unwrap())),
                None => {
                    let ioe: io::Error = io::ErrorKind::NotFound.into();
                    Err(ioe)
                }
            }
        })?;
        ws::start(r, Ws { plc_conn, version })
    }
}

#[inline(always)]
fn get_size_by_key<T: ToString>(version: &AdsVersion, k: &T) -> usize {
    let key_guard: &String = &*version.search_index.get(&k.to_string()).unwrap();
    let value = &*version.map.get(key_guard).unwrap();
    value.len() as usize
}

impl Actor for Ws {
    type Context = ws::WebsocketContext<Self, Arc<WsState>>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let state: &WsState = ctx.state();
        let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");
        let mem = AdsMemory {
            ST_ADS_TO_BC: vec![0u8; get_size_by_key(version, &"ST_ADS_TO_BC")],
            ST_ADS_FROM_BC: vec![0u8; get_size_by_key(version, &"ST_ADS_FROM_BC")],
            ST_RETAIN_DATA: vec![0u8; get_size_by_key(version, &"ST_RETAIN_DATA")],
        };
        state.data.insert(self.plc_conn, mem);
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Ws {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        let state = ctx.state().clone();
        let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");
        let mem = &mut *state.data.get_mut(&self.plc_conn).unwrap();
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => {
                let t1 = Instant::now();

                match text.chars().position(|e| e == ':') {
                    Some(_) => {
                        if let Ok(mutation) = serde_json::from_str::<Value>(&text) {
                            if let Value::Object(obj) = mutation {
                                let _: Vec<()> = obj
                                    .into_iter()
                                    .filter_map(|param: (String, Value)| {
                                        let (k, data) = param;
                                        if let Some(key_guard) =
                                            version.search_index.get(&k.trim().to_string())
                                        {
                                            let value: &AdsType = &*version.map.get(&*key_guard).unwrap();
                                            let mut m = mem.by_str(k.trim());
                                            let v =
                                                value.as_data_struct(&mut (&m[..]), &version.map);
                                            let _ = value.to_writer(
                                                &merge(v, data),
                                                &mut m.as_mut_slice(),
                                                &version.map,
                                            );
                                        }
                                        None
                                    })
                                    .collect();
                            }
                        }
                    }
                    None => {
                        let schema = schema_parser(&text).unwrap();
                        if let Schema::Root(v) = &schema {
                            ctx.text(
                                to_string(
                                    &schema
                                        .as_schema(&Value::Object(
                                            v.iter()
                                                .filter_map(|f| {
                                                    let name = match f {
                                                        Schema::Obj(n, _) => n,
                                                        Schema::Tag(n) => n,
                                                        Schema::Root(_) => unreachable!(),
                                                    };
                                                    match version
                                                        .search_index
                                                        .get(&name.trim().to_string())
                                                    {
                                                        Some(key_guard) => {
                                                            let value: &AdsType = &*version.map.get(&*key_guard).unwrap();
                                                            let v = value.as_data_struct(
                                                                &mut (&mem.by_str(name.trim())[..]),
                                                                &version.map,
                                                            );
                                                            Some((name.to_string(), v))
                                                        }
                                                        None => None,
                                                    }
                                                })
                                                .collect(),
                                        ))
                                        .unwrap(),
                                ).unwrap(),
                            );
                        }
                    }
                }
                println!("{:?}", t1.elapsed());
            }
            _ => (),
        }
    }
}
