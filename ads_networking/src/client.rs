use super::codec::{self, types::AdsCommand};
use actix::prelude::*;
use ads_types::Symbol;
use byteorder::{ByteOrder, LittleEndian};
use rand::{self, Rng};
use std::io;
use std::time::Duration;
use tokio_io::io::WriteHalf;
use tokio_tcp::TcpStream;

#[derive(Debug)]
pub struct AdsStructMap {
	pub st_ads_to_bc: Symbol,
	pub st_retain_data: Symbol,
}

pub struct AdsClient {
	pub framed: actix::io::FramedWrite<WriteHalf<TcpStream>, codec::AdsClientCodec>,
	pub source: [u8; 8],
	pub target: [u8; 8],
	pub index_offset: u32,
	pub count: u32,
	pub struct_map: AdsStructMap,
}

#[derive(Debug)]
pub enum AdsClientCommand {
	ReadToBc,
	ReadRetain,
	WriteToBc,
	WriteRetain,
}

impl Message for AdsClientCommand {
	type Result = ();
}

impl Actor for AdsClient {
	type Context = Context<Self>;

	fn started(&mut self, ctx: &mut Self::Context) {
		println!("ads_client started");
		self.hb(ctx);
	}

	fn stopped(&mut self, _: &mut Self::Context) {
		println!("stopped");
	}
}

impl AdsClient {
	fn hb(&mut self, ctx: &mut Context<Self>) {
		let c = self.count;
		self.count += 1;
		let mut d = vec![0u8; 4];
		LittleEndian::write_u32(&mut d, c);
		let wr = codec::AmsTcpHeader {
			length: 32 + 12 + 4,
			header: codec::AmsHeader {
				source: self.source,
				target: self.target,
				command_id: 3,
				inv_id: rand::thread_rng().gen(),
				state_flags: 4,
				data: codec::AdsWriteReq {
					index_group: self.struct_map.st_ads_to_bc.index_group,
					index_offset: self.index_offset,
					length: 4,
					data: d,
				},
			},
		};
		self.framed.write(codec::AdsPacket::WriteReq(wr));
		ctx.run_later(Duration::new(5, 0), |act, ctx| {
			act.hb(ctx);
		});
	}
}

impl actix::io::WriteHandler<io::Error> for AdsClient {}

impl Handler<AdsClientCommand> for AdsClient {
	type Result = ();

	fn handle(&mut self, msg: AdsClientCommand, ctx: &mut Context<Self>) {
		println!("h: {:?} {:?}", msg, ctx.state());
	}
}

impl StreamHandler<codec::AdsPacket, io::Error> for AdsClient {
	fn handle(&mut self, msg: codec::AdsPacket, _: &mut Context<Self>) {
		use super::codec::AdsPacket::*;
		match msg {
			ReadReq(r) => {
				println!("read_req: {:?}", r);
				self.framed.write(codec::AdsPacket::ReadRes(r.gen_res()));
			}
			ReadRes(r) => {
				println!("read_res: {:?}", r);
			}
			WriteReq(w) => {
				self.framed.write(codec::AdsPacket::WriteRes(w.gen_res()));
				println!("write_req: {:?}", w);
			}
			WriteRes(_w) => {}
		}
	}
}
