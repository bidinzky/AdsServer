use super::json_diff::{merge, schema_parser, Schema};
use super::networking::{AdsClientCommand, AdsClientToWs, AdsPacket, Client, ToPlcConn};
use super::settings::PlcSetting;
use super::types::{AdsType, AdsVersion};
use actix::prelude::*;
use actix::AsyncContext;
use actix_web::{ws, Error, HttpRequest, HttpResponse};
use chashmap::CHashMap;
use serde_json::{self, to_string, Value};
use std::fmt;
use std::io;
use std::sync::{Arc, RwLock, RwLockReadGuard};
use std::time::Instant;
use ws_ads::AdsToWsMultiplexer;

pub enum WsToAdsClient {
    Register(Addr<Ws>),
    Unregister(Addr<Ws>),
    Resolve(Schema),
    Mutation(Value),
    Subscription(Schema),
}

impl fmt::Debug for WsToAdsClient {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            WsToAdsClient::Register(_) => write!(f, "Register"),
            WsToAdsClient::Unregister(_) => write!(f, "Unregister"),
            WsToAdsClient::Resolve(rest) => write!(f, "R: {:?}", rest),
            WsToAdsClient::Mutation(rest) => write!(f, "M: {:?}", rest),
            WsToAdsClient::Subscription(rest) => write!(f, "S: {:?}", rest),
        }
    }
}

impl Message for WsToAdsClient {
    type Result = ();
}

#[derive(Debug)]
#[allow(non_snake_case)]
struct AdsMemory {
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
    config: RwLock<Vec<PlcSetting>>,
    sender: CHashMap<[u8; 8], Addr<AdsToWsMultiplexer>>,
}

impl WsState {
    pub fn new(
        config: RwLock<Vec<PlcSetting>>,
        sender: CHashMap<[u8; 8], Addr<AdsToWsMultiplexer>>,
    ) -> Self {
        WsState { config, sender }
    }
    pub fn config<'a>(&'a self) -> RwLockReadGuard<'a, Vec<PlcSetting>> {
        self.config.read().unwrap()
    }
}

pub struct Ws {
    plc_conn: [u8; 8],
    version: u32,
    c: Option<Addr<AdsToWsMultiplexer>>,
}

impl Handler<AdsPacket> for Ws {
    type Result = ();
    fn handle(&mut self, m: AdsPacket, ctx: &mut Self::Context) -> Self::Result {
        println!("ws: {:?}", m);
    }
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
                Some(c) => Ok((c.version, (net_id, port).as_plc_conn())),
                None => {
                    let ioe: io::Error = io::ErrorKind::NotFound.into();
                    Err(ioe)
                }
            }
        })?;
        ws::start(
            &r,
            Ws {
                plc_conn,
                version,
                c: None,
            },
        )
    }
}

impl Actor for Ws {
    type Context = ws::WebsocketContext<Ws, Arc<WsState>>;
    fn started(&mut self, ctx: &mut Self::Context) {
        let a = ctx.state().sender.get(&self.plc_conn).unwrap().clone();
        a.do_send(WsToAdsClient::Register(ctx.address()));
        self.c = Some(a);
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        if let Some(ref channel) = self.c {
            channel.do_send(WsToAdsClient::Unregister(ctx.address()));
        }
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Ws {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        let state = ctx.state().clone();
        //let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");
        //let mem = &mut *state.data.get_mut(&self.plc_conn).unwrap();
        let sender = &mut *state.sender.get_mut(&self.plc_conn).unwrap();
        //sender.do_send(AdsClientCommand::ReadToBc);
        //sender.do_send(WsClient::Register(ctx.address()));
        //ctx.address().do_send(AdsClientCommand::ReadToBc(10));
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => {
                let t1 = Instant::now();
                match text.chars().position(|e| e == ':') {
                    Some(_) => {
                        //Mutation
                        if let Ok(mutation) = serde_json::from_str::<Value>(&text) {
                            sender.do_send(WsToAdsClient::Mutation(mutation));
                            /**/
                        }
                    }
                    None => {
                        // GET
                        let v = schema_parser(&text).unwrap();
                        sender.do_send(WsToAdsClient::Resolve(v));
                        /**/
                    }
                }
                println!("{:?}", t1.elapsed());
            }
            _ => (),
        }
    }
}
