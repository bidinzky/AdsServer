use super::codec::{self, types::AdsCommand, AdsPacket, AmsTcpHeader};
use actix::prelude::*;
use byteorder::{ByteOrder, LittleEndian};
use rand::{self, Rng};
use std::io;
use std::time::Duration;
use tokio_io::io::WriteHalf;
use tokio_tcp::TcpStream;
use types::Symbol;
use ws::{Ws, WsToAdsClient};

pub enum AdsClientToWs {
	ReadResult(codec::AdsPacket),
	WriteData(codec::AdsPacket),
}

impl Message for AdsClientToWs {
	type Result = ();
}

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
	pub ws_clients: Vec<Addr<Ws>>,
}

impl Handler<WsToAdsClient> for AdsClient {
	type Result = ();
	fn handle(&mut self, msg: WsToAdsClient, ctx: &mut Self::Context) -> Self::Result {
		match msg {
			WsToAdsClient::Register(m) => if !self.ws_clients.contains(&m) {
				self.ws_clients.push(m);
			},
			WsToAdsClient::Unregister(m) => {
				if let Some(i) = self.ws_clients.iter().position(|x| *x == m) {
					self.ws_clients.remove(i);
				}
			}
		}
	}
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
		self.hb(ctx);
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
	//fn gen_read_request<T>(&mut self, index_group: u32, index_offset: u32, length: u32) -> AmsTcpHeader<
	fn hb(&mut self, ctx: &mut Context<Self>) {
		let c = self.count;
		self.count += 1;
		let mut d = vec![0u8; 4];
		LittleEndian::write_u32(&mut d, c);
		let idx_grp = self.struct_map.st_ads_to_bc.index_group;
		let idx_offs = self.index_offset;
		let wr = self.gen_write_request(idx_grp, idx_offs, d);
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
		println!("h: {:?}", msg);
		match msg {
			AdsClientCommand::ReadRetain(len) => {
				let d = AdsPacket::ReadReq(self.gen_read_request(
					self.struct_map.st_retain_data.index_group,
					self.struct_map.st_retain_data.index_offset,
					len,
				));
				self.framed.write(d);
			}
			AdsClientCommand::ReadToBc(len) => {
				let d = AdsPacket::ReadReq(self.gen_read_request(
					self.struct_map.st_ads_to_bc.index_group,
					self.struct_map.st_ads_to_bc.index_offset,
					len,
				));
				self.framed.write(d);
			}
			AdsClientCommand::WriteRetain(d) => {
				let d = AdsPacket::WriteReq(self.gen_write_request(
					self.struct_map.st_retain_data.index_group,
					self.struct_map.st_retain_data.index_offset,
					d,
				));
				self.framed.write(d);
			}
			AdsClientCommand::WriteToBc(d) => {
				let d = AdsPacket::WriteReq(self.gen_write_request(
					self.struct_map.st_ads_to_bc.index_group,
					self.struct_map.st_ads_to_bc.index_offset,
					d,
				));
				self.framed.write(d);
			}
		}
	}
}

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
		send_to_ws(&self.ws_clients, msg);
	}
}
