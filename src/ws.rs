use super::settings::PlcSetting;
use super::xml_to_struct::types::AdsVersion;
use actix::prelude::*;
use actix_web::{ws, Error, HttpRequest, HttpResponse};
use chashmap::CHashMap;
use networking::ToPlcConn;
use std::io;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct WsState {
    pub map: Arc<CHashMap<u32, AdsVersion>>,
    pub config: Arc<RwLock<Vec<PlcSetting>>>,
}

#[derive(Debug)]
pub struct Ws {
    plc_conn: [u8; 8],
    version: u32,
}

impl Ws {
    pub fn ws_index(r: HttpRequest<WsState>) -> Result<HttpResponse, Error> {
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

impl Actor for Ws {
    type Context = ws::WebsocketContext<Self, WsState>;
    fn started(&mut self, ctx: &mut Self::Context) {
        println!("started, {:#?}", self);
    }
}

impl StreamHandler<ws::Message, ws::ProtocolError> for Ws {
    fn handle(&mut self, msg: ws::Message, ctx: &mut Self::Context) {
        println!("{:?}", msg);
        match msg {
            ws::Message::Ping(msg) => ctx.pong(&msg),
            ws::Message::Text(text) => ctx.text(text),
            ws::Message::Binary(bin) => ctx.binary(bin),
            _ => (),
        }
    }
}
