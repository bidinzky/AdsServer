use super::codec::{self, types::AdsCommand, AdsPacket, AmsTcpHeader};
use actix::dev::{MessageResponse, ResponseChannel};
use actix::fut::wrap_future;
use actix::prelude::*;
use byteorder::{ByteOrder, LittleEndian};
use futures::oneshot;
use futures::sync::{mpsc, oneshot};
use futures::{Future, Poll, Stream};
use rand::{self, Rng};
use std::collections::HashMap;
use std::fmt::Debug;
use std::io;
use std::marker::PhantomData;
use std::time::Duration;
use tokio_io::io::WriteHalf;
use tokio_tcp::TcpStream;
use ws::{Ws, WsToAdsClient};
use ws_ads::AdsToWsMultiplexer;

pub enum WsMultiplexerRegister {
    Register(Addr<AdsToWsMultiplexer>),
    Unregister,
}

impl Message for WsMultiplexerRegister {
    type Result = Option<mpsc::Receiver<codec::AdsWriteReq>>;
}

impl Handler<WsMultiplexerRegister> for AdsClient {
    type Result = Option<mpsc::Receiver<codec::AdsWriteReq>>;

    fn handle(&mut self, msg: WsMultiplexerRegister, _: &mut Self::Context) -> Self::Result {
        match msg {
            WsMultiplexerRegister::Register(a) => {
                self.ws_ads = Some(a);
                let (tx, rx) = mpsc::channel(1);
                self.write_request_sender = Some(tx);
                Some(rx)
            }
            WsMultiplexerRegister::Unregister => {
                self.ws_ads = None;
                None
            }
        }
    }
}

pub struct AdsClient {
    framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::AdsClientCodec>,
    source: [u8; 8],
    target: [u8; 8],
    ws_ads: Option<Addr<AdsToWsMultiplexer>>,
    request_map: HashMap<u32, oneshot::Sender<AmsTcpHeader<codec::AdsReadRes>>>,
    write_request_sender: Option<mpsc::Sender<codec::AdsWriteReq>>,
}

impl AdsClient {
    pub fn new(
        framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::AdsClientCodec>,
        source: [u8; 8],
        target: [u8; 8],
    ) -> Self {
        AdsClient {
            framed,
            source,
            target,
            ws_ads: None,
            request_map: HashMap::new(),
            write_request_sender: None,
        }
    }
}

impl Actor for AdsClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("ads_client started");
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        println!("ads_client: stopped");
    }
}

impl AdsClient {
    fn gen_request<T>(&self, command_id: u16, state_flags: u16, data: T) -> (AmsTcpHeader<T>, u32)
    where
        T: AdsCommand,
    {
        let r = rand::thread_rng().gen();
        (
            codec::AmsTcpHeader {
                length: 32 + data.size() as u32,
                header: codec::AmsHeader {
                    source: self.source,
                    target: self.target,
                    command_id,
                    inv_id: r,
                    state_flags,
                    data,
                },
            },
            r,
        )
    }

    fn gen_write_request(
        &self,
        index_group: u32,
        index_offset: u32,
        data: Vec<u8>,
    ) -> (AmsTcpHeader<codec::AdsWriteReq>, u32) {
        self.gen_request(
            3,
            4,
            codec::AdsWriteReq {
                index_group,
                index_offset,
                length: data.len() as u32,
                data,
            },
        )
    }

    fn gen_read_request(
        &self,
        index_group: u32,
        index_offset: u32,
        length: u32,
    ) -> (AmsTcpHeader<codec::AdsReadReq>, u32) {
        self.gen_request(
            2,
            4,
            codec::AdsReadReq {
                index_group,
                index_offset,
                length,
            },
        )
    }
}

impl actix::io::WriteHandler<io::Error> for AdsClient {}

impl StreamHandler<codec::AdsPacket, io::Error> for AdsClient {
    fn handle(&mut self, msg: codec::AdsPacket, _: &mut Context<Self>) {
        use super::codec::AdsPacket::*;
        match msg {
            ReadReq(r) => {
                println!("read_req: {:?}", r);
                self.framed.write(codec::AdsPacket::ReadRes(r.gen_res()));
            }
            ReadRes(r) => {
                let rres = self.request_map.remove(&r.header.inv_id).unwrap();
                let _ = rres.send(r);
                //send_to_ws(&self.ws_ads, AdsClientToWs::ReadResult(r));
            }
            WriteReq(w) => {
                self.framed.write(codec::AdsPacket::WriteRes(w.gen_res()));
                if let Some(ref mut tx) = &mut self.write_request_sender {
                    let _ = tx.try_send(w.header.data);
                }
                //println!("write_req: {:?}", w);
                //send_to_ws(&self.ws_ads, AdsClientToWs::WriteData(w));
            }
            WriteRes(_w) => {}
        }
        //send_to_ws(&self.ws_clients, msg);
    }
}

impl Handler<codec::AdsReadReq> for AdsClient {
    type Result = Box<Future<Item = codec::AdsReadRes, Error = ()>>;

    fn handle(
        &mut self,
        msg: codec::AdsReadReq,
        _: &mut Self::Context,
    ) -> Box<Future<Item = codec::AdsReadRes, Error = ()>> {
        let (req, inv) = self.gen_request(2, 4, msg);
        let (tx, rx) = oneshot();
        self.request_map.insert(inv, tx);
        self.framed.write(AdsPacket::ReadReq(req));
        Box::new(rx.map_err(|_| println!("error")).map(|f| f.header.data))
    }
}

impl Handler<codec::AdsWriteReq> for AdsClient {
    type Result = ();

    fn handle(&mut self, msg: codec::AdsWriteReq, _: &mut Self::Context) -> Self::Result {
        let (req, _) = self.gen_request(3, 4, msg);
        self.framed.write(AdsPacket::WriteReq(req));
    }
}

/*impl<T: 'static> Handler<T> for AdsClient
where
    T: codec::AdsCommand + Message<Result = ()> + Debug,cls

{
    type Result = ();

    fn handle(&mut self, msg: T, _: &mut Self::Context) {
        println!("wrong");
        use std::any::Any;
        let m = Box::new(msg) as Box<Any>;
        let d = match m.downcast::<codec::AdsReadReq>() {
            Ok(rr) => {
                let (req, inv) = ;
                AdsPacket::ReadReq(req)
            }
            Err(m) => match m.downcast::<codec::AdsWriteReq>() {
                Ok(wr) => {
                    let (req, inv) = self.gen_request(3, 4, *wr);
                    AdsPacket::WriteReq(req)
                }
                _ => unreachable!(),
            },
        };
        self.framed.write(d);
    }
}
*/
