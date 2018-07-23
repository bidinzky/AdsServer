use super::codec::{self, types::AdsCommand, AdsPacket, AmsTcpHeader};
use actix::prelude::*;
use byteorder::{ByteOrder, LittleEndian};
use rand::{self, Rng};
use std::fmt::Debug;
use std::io;
use std::time::Duration;
use tokio_io::io::WriteHalf;
use tokio_tcp::TcpStream;
use ws::{Ws, WsToAdsClient};

pub enum AdsClientToWs {
    ReadResult(codec::AdsPacket),
    WriteData(codec::AdsPacket),
}

impl Message for AdsClientToWs {
    type Result = ();
}

pub struct AdsClient {
    pub framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::AdsClientCodec>,
    pub source: [u8; 8],
    pub target: [u8; 8],
}

#[derive(Debug)]
pub enum AdsClientCommand {
    ReadToBc(u32),
    ReadRetain(u32),
    WriteToBc(Vec<u8>),
    WriteRetain(Vec<u8>),
}

impl Message for AdsClientCommand {
    type Result = ();
}

impl Actor for AdsClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        println!("ads_client started");
        //self.hb(ctx);
    }

    fn stopped(&mut self, _: &mut Self::Context) {
        println!("stopped");
    }
}

impl AdsClient {
    fn gen_request<T>(&self, command_id: u16, state_flags: u16, data: T) -> AmsTcpHeader<T>
    where
        T: AdsCommand,
    {
        codec::AmsTcpHeader {
            length: 32 + data.size() as u32,
            header: codec::AmsHeader {
                source: self.source,
                target: self.target,
                command_id,
                inv_id: rand::thread_rng().gen(),
                state_flags,
                data,
            },
        }
    }

    fn gen_write_request(
        &self,
        index_group: u32,
        index_offset: u32,
        data: Vec<u8>,
    ) -> AmsTcpHeader<codec::AdsWriteReq> {
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
    ) -> AmsTcpHeader<codec::AdsReadReq> {
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

/**/

fn send_to_ws(v: &[Addr<Ws>], data: codec::AdsPacket) {
    for a in v {
        a.do_send(data.clone());
    }
}

impl StreamHandler<codec::AdsPacket, io::Error> for AdsClient {
    fn handle(&mut self, msg: codec::AdsPacket, _: &mut Context<Self>) {
        use super::codec::AdsPacket::*;
        match &msg {
            ReadReq(r) => {
                println!("read_req: {:?}", r);
                //self.framed.write(codec::AdsPacket::ReadRes(r.gen_res()));
            }
            ReadRes(r) => {
                println!("read_res: {:?}", r);
            }
            WriteReq(w) => {
                self.framed.write(codec::AdsPacket::WriteRes(w.gen_res()));
                //println!("write_req: {:?}", w);
            }
            WriteRes(_w) => {}
        }
        //send_to_ws(&self.ws_clients, msg);
    }
}

impl<T: 'static> Handler<T> for AdsClient
where
    T: codec::AdsCommand + Message<Result = ()> + Debug,
{
    type Result = ();

    fn handle(&mut self, msg: T, _: &mut Self::Context) {
        use std::any::Any;
        let m = Box::new(msg) as Box<Any>;
        let d = match m.downcast::<codec::AdsReadReq>() {
            Ok(rr) => AdsPacket::ReadReq(self.gen_request(2, 4, *rr)),
            Err(m) => match m.downcast::<codec::AdsWriteReq>() {
                Ok(wr) => AdsPacket::WriteReq(self.gen_request(3, 4, *wr)),
                _ => unreachable!(),
            },
        };
        self.framed.write(d);
    }
}
