use super::json_diff::{merge, schema_parser, Schema};
use super::networking::{AdsPacket, Client, ToPlcConn};
use super::settings::PlcSetting;
use super::types::{AdsType, AdsVersion};
use actix::fut::wrap_future;
use actix::prelude::*;
use actix::ActorFuture;
use actix::AsyncContext;
use actix_web::{ws, Error, HttpRequest, HttpResponse};
use chashmap::CHashMap;
use futures::Future;
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

pub struct AdsToWsClient(pub Arc<String>);

impl Message for AdsToWsClient {
    type Result = ();
}

impl Handler<AdsToWsClient> for Ws {
    type Result = ();
    fn handle(&mut self, m: AdsToWsClient, ctx: &mut Self::Context) -> Self::Result {
        ctx.text(m.0);
    }
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
    type Result = Result<String, ()>;
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
        let sender = &mut *state.sender.get_mut(&self.plc_conn).unwrap();
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => match text.chars().position(|e| e == ':') {
                Some(_) => {
                    if let Ok(mutation) = serde_json::from_str::<Value>(&text) {
                        sender.do_send(WsToAdsClient::Mutation(mutation));
                    }
                }
                None => {
                    let v = schema_parser(&text).unwrap();
                    ctx.spawn(
                        wrap_future(sender.send(WsToAdsClient::Resolve(v)))
                            .map(|f, _, ctx: &mut Self::Context| ctx.text(f.unwrap()))
                            .map_err(|_, _, _| println!("error on {} {}", line!(), file!())),
                    );
                }
            },
            _ => (),
        }
    }
}
