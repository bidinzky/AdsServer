use actix::prelude::*;

mod client;
mod codec;

use client::AdsClient as Client;
use std::process;

use futures::Future;
use tokio_codec::FramedRead;
use tokio_io::AsyncRead;
use tokio_tcp::TcpStream;

pub fn CreateClient<T: std::net::ToSocketAddrs>(
	addr: T,
	source: Vec<u8>,
	target: Vec<u8>,
	idx_grp: u32,
	idx_offs: u32,
) -> impl Future<Item = (), Error = ()> {
	TcpStream::connect(&addr.to_socket_addrs().unwrap().next().unwrap())
		.and_then(move |stream| {
			Client::create(move |ctx| {
				let (r, w) = stream.split();
				ctx.add_stream(FramedRead::new(r, codec::AdsClientCodec));
				Client {
					framed: actix::io::FramedWrite::new(w, codec::AdsClientCodec, ctx),
					target,
					source,
					index_group: idx_grp,
					index_offset: idx_offs,
					count: 0,
				}
			});

			futures::future::ok(())
		})
		.map_err(|e| {
			println!("Can not connect to server: {}", e);
			process::exit(1)
		})
}
