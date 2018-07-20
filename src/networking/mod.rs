use actix::prelude::*;

mod client;
mod codec;

pub use self::client::AdsClient as Client;
use std::process;

use futures::{future, Future};
use std::net::ToSocketAddrs;
use tokio_codec::FramedRead;
use tokio_io::AsyncRead;
use tokio_tcp::TcpStream;

pub use self::client::*;
pub use self::codec::types::*;
pub use self::codec::*;

pub trait ToPlcConn {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]>;
}

impl<T> ToPlcConn for (T, u16)
where
    T: AsRef<str>,
{
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        let net_id: Vec<_> = self
            .0
            .as_ref()
            .split('.')
            .map(|x| u8::from_str_radix(x, 10).unwrap())
            .collect();
        let mut d = [0u8; 8];
        d[..6].clone_from_slice(&net_id[..6]);
        d[7] = ((self.1 >> 8) & 0xff) as u8;
        d[6] = (self.1 & 0xff) as u8;
        Some(d)
    }
}

pub fn create_client<T: ToSocketAddrs>(
    addr: T,
    target: &impl ToPlcConn,
    source: &impl ToPlcConn,
    idx_offs: u32,
    struct_map: AdsStructMap,
) -> impl Future<Item = Addr<Client>, Error = ()> {
    let target = target.try_into_plc_conn().expect("expected plc conn");
    let source = source.try_into_plc_conn().expect("expected plc conn");
    TcpStream::connect(&addr.to_socket_addrs().unwrap().next().unwrap())
        .and_then(move |stream| {
            future::ok(Client::create(move |ctx| {
                let (r, w) = stream.split();
                ctx.add_stream(FramedRead::new(r, codec::AdsClientCodec));
                Client {
                    framed: actix::io::FramedWrite::new(w, codec::AdsClientCodec, ctx),
                    target,
                    source,
                    index_offset: idx_offs,
                    count: 0,
                    struct_map,
                    ws_clients: vec![],
                }
            }))
        })
        .map_err(|e| {
            println!("Can not connect to server: {}", e);
            process::exit(1)
        })
}
