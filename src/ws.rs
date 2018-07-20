use super::settings::PlcSetting;
use actix::prelude::*;
use actix_web::{ws, Error, HttpRequest, HttpResponse};
use ads_types::{AdsType, AdsVersion};
use chashmap::CHashMap;
use json_diff::{merge, schema_parser, Schema};
use networking::ToPlcConn;
use networking::{AdsClientCommand, Client};
use serde_json::{self, to_string, Value};
use std::io;
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct AdsMemory {
    ST_ADS_TO_BC: Value,
    ST_ADS_FROM_BC: Value,
    ST_RETAIN_DATA: Value,
}

impl AdsMemory {
    pub fn by_str(&mut self, s: &str) -> &mut Value {
        match s.trim() {
            "ST_ADS_TO_BC" => &mut self.ST_ADS_TO_BC,
            "ST_ADS_FROM_BC" => &mut self.ST_ADS_FROM_BC,
            "ST_RETAIN_DATA" => &mut self.ST_RETAIN_DATA,
            _ => unreachable!(),
        }
    }

    pub fn update(&mut self, s: &str, v: Value) {
        match s.trim() {
            "ST_ADS_TO_BC" => self.ST_ADS_TO_BC = merge(self.ST_ADS_TO_BC.clone(), v),
            "ST_ADS_FROM_BC" => self.ST_ADS_FROM_BC = merge(self.ST_ADS_FROM_BC.clone(), v),
            "ST_RETAIN_DATA" => self.ST_RETAIN_DATA = merge(self.ST_RETAIN_DATA.clone(), v),
            _ => unreachable!(),
        }
    }
}

pub struct WsState {
    pub map: CHashMap<u32, AdsVersion>,
    pub config: RwLock<Vec<PlcSetting>>,
    pub data: CHashMap<[u8; 8], AdsMemory>,
    pub sender: CHashMap<[u8; 8], Addr<Client>>,
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
        ws::start(&r, Ws { plc_conn, version })
    }
}

fn init_st_ads_to_bc<T: ToString>(version: &AdsVersion, k: &T) -> Value {
    let key_guard: &String = &*version.search_index.get(&k.to_string()).unwrap();
    let value: &AdsType = &*version.map.get(&*key_guard).unwrap();
    value.as_data_struct(&mut &vec![0u8; value.len() as usize][..], &version.map)
}

impl Actor for Ws {
    type Context = ws::WebsocketContext<Ws, Arc<WsState>>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let state: &WsState = ctx.state();
        let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");

        let mem = AdsMemory {
            ST_ADS_TO_BC: init_st_ads_to_bc(version, &"ST_ADS_TO_BC"),
            ST_ADS_FROM_BC: init_st_ads_to_bc(version, &"ST_ADS_FROM_BC"),
            ST_RETAIN_DATA: init_st_ads_to_bc(version, &"ST_RETAIN_DATA"),
        };
        state.data.insert(self.plc_conn, mem);
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Ws {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        let state = ctx.state().clone();
        let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");
        let mem = &mut *state.data.get_mut(&self.plc_conn).unwrap();
        let sender = &mut *state.sender.get_mut(&self.plc_conn).unwrap();
        //sender.do_send(AdsClientCommand::ReadToBc);
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => {
                let t1 = Instant::now();
                match text.chars().position(|e| e == ':') {
                    Some(_) => {
                        //Mutation
                        if let Ok(mutation) = serde_json::from_str::<Value>(&text) {
                            if let Value::Object(obj) = mutation {
                                let _: Vec<()> = obj
                                    .into_iter()
                                    .filter_map(|param: (String, Value)| {
                                        let (k, data) = param;
                                        if let Some(_key_guard) =
                                            version.search_index.get(&k.trim().to_string())
                                        {
                                            //let value: &AdsType = &*version.map.get(&*key_guard).unwrap();
                                            mem.update(k.trim(), data);
                                            match k.trim() {
                                                "ST_ADS_TO_BC" => {
                                                    sender.do_send(AdsClientCommand::WriteToBc);
                                                }
                                                "ST_RETAIN_DATA" => {
                                                    sender.do_send(AdsClientCommand::WriteRetain);
                                                }
                                                _ => unreachable!(),
                                            }
                                            /*let mut m: &mut Value = mem.by_str(k.trim());
                                            state
                                                .data
                                                .insert(self.plc_conn, merge(m.clone(), data));*/
                                        }
                                        None
                                    })
                                    .collect();
                            }
                        }
                    }
                    None => {
                        // GET
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
                                                        Some(_key_guard) => {
                                                            //let value: &AdsType = &*version.map.get(&*key_guard).unwrap();
                                                            let v = mem.by_str(name.trim());
                                                            match name.trim() {
                                                                "ST_ADS_TO_BC" => {
                                                                    sender.do_send(
                                                                        AdsClientCommand::ReadToBc,
                                                                    );
                                                                }
                                                                "ST_RETAIN_DATA" => {
                                                                    sender.do_send(AdsClientCommand::ReadRetain);
                                                                }
                                                                _ => unreachable!(),
                                                            }
                                                            Some((name.to_string(), v.clone()))
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
