use actix::prelude::*;
use byteorder::{ByteOrder, LittleEndian};
use json_diff::merge;
use networking::{AdsClientCommand, AdsClientToWs, AdsPacket, AdsReadReq, AdsWriteReq, Client};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use types::Symbol;
use types::{AdsType, AdsVersion};
use ws::{Ws, WsState, WsToAdsClient};

#[derive(Debug)]
pub struct AdsMemoryValue {
	pub data: Value,
	pub vec: Vec<u8>,
}

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct AdsMemory {
	pub ST_ADS_TO_BC: AdsMemoryValue,
	pub ST_ADS_FROM_BC: AdsMemoryValue,
	pub ST_RETAIN_DATA: AdsMemoryValue,
}

impl AdsMemory {
	pub fn by_str_mut(&mut self, s: &str) -> &mut AdsMemoryValue {
		match s.trim() {
			"ST_ADS_TO_BC" => &mut self.ST_ADS_TO_BC,
			"ST_ADS_FROM_BC" => &mut self.ST_ADS_FROM_BC,
			"ST_RETAIN_DATA" => &mut self.ST_RETAIN_DATA,
			_ => unreachable!(),
		}
	}

	pub fn by_str(&self, s: &str) -> &AdsMemoryValue {
		match s.trim() {
			"ST_ADS_TO_BC" => &self.ST_ADS_TO_BC,
			"ST_ADS_FROM_BC" => &self.ST_ADS_FROM_BC,
			"ST_RETAIN_DATA" => &self.ST_RETAIN_DATA,
			_ => unreachable!(),
		}
	}

	pub fn update(&mut self, s: &str, v: Value) {
		match s.trim() {
			"ST_ADS_TO_BC" => self.ST_ADS_TO_BC.data = merge(self.ST_ADS_TO_BC.data.clone(), v),
			"ST_ADS_FROM_BC" => {
				self.ST_ADS_FROM_BC.data = merge(self.ST_ADS_FROM_BC.data.clone(), v)
			}
			"ST_RETAIN_DATA" => {
				self.ST_RETAIN_DATA.data = merge(self.ST_RETAIN_DATA.data.clone(), v)
			}
			_ => unreachable!(),
		}
	}
}

#[derive(Debug)]
pub struct AdsStructMap {
	pub st_ads_to_bc: Symbol,
	pub st_retain_data: Symbol,
}

pub struct AdsToWsMultiplexer {
	pub ws_clients: Vec<Addr<Ws>>,
	pub client: Addr<Client>,
	pub data: AdsMemory,
	pub version: Arc<AdsVersion>,
	pub struct_map: AdsStructMap,
	count: u32,
}

impl AdsToWsMultiplexer {
	pub fn new(
		client: Addr<Client>,
		data: AdsMemory,
		version: Arc<AdsVersion>,
		struct_map: AdsStructMap,
	) -> Self {
		AdsToWsMultiplexer {
			ws_clients: Vec::new(),
			client,
			data,
			version,
			struct_map,
			count: 0,
		}
	}
}

impl Actor for AdsToWsMultiplexer {
	type Context = Context<Self>;
	fn started(&mut self, ctx: &mut Self::Context) {
		println!("ads_client_mutliplexer started");
		self.hb(ctx);
	}

	fn stopped(&mut self, _: &mut Self::Context) {
		println!("ads_client_mutliplexer stopped");
	}
}

impl AdsToWsMultiplexer {
	fn hb(&mut self, ctx: &mut Context<Self>) {
		let c = self.count;
		self.count += 1;
		{
			let d = &mut self.data.ST_ADS_TO_BC.vec[16..20];
			LittleEndian::write_u32(d, c);
		}
		if let Some(key) = self.version.search_index.get(&"ST_ADS_TO_BC".to_string()) {
			let ty: &AdsType = &*self.version.map.get(&*key).unwrap();
			self.data.ST_ADS_TO_BC.data = ty.as_data_struct(
				&mut self.data.ST_ADS_TO_BC.vec.as_slice(),
				&self.version.map,
			);
		}
		let wr = AdsWriteReq {
			index_group: self.struct_map.st_ads_to_bc.index_group,
			index_offset: self.struct_map.st_ads_to_bc.index_offset + 16,
			length: 4,
			data: self.data.ST_ADS_TO_BC.vec[16..20].to_vec(),
		};
		self.client.do_send(wr);
		ctx.run_later(Duration::new(5, 0), |act, ctx| {
			act.hb(ctx);
		});
	}
}

impl Handler<WsToAdsClient> for AdsToWsMultiplexer {
	type Result = ();
	fn handle(&mut self, msg: WsToAdsClient, ctx: &mut Self::Context) -> Self::Result {
		//let version: &AdsVersion = &*state.map.get(&self.version).expect("unknown version");
		//let mem = &mut *state.data.get_mut(&self.plc_conn).unwrap();
		match msg {
			WsToAdsClient::Register(m) => if !self.ws_clients.contains(&m) {
				self.ws_clients.push(m);
			},
			WsToAdsClient::Unregister(m) => {
				if let Some(i) = self.ws_clients.iter().position(|x| *x == m) {
					self.ws_clients.remove(i);
				}
			}
			WsToAdsClient::Mutation(mutation) => {
				let version = &self.version;
				let mut mem = &mut self.data;
				let client = &self.client;
				let struct_map = &self.struct_map;
				if let Value::Object(obj) = mutation {
					let _: Vec<()> = obj
						.into_iter()
						.filter_map(|param: (String, Value)| {
							let (k, data) = param;
							if let Some(key_guard) = version.search_index.get(&k.trim().to_string())
							{
								let ty: &AdsType = &*version.map.get(&*key_guard).unwrap();
								mem.update(k.trim(), data);
								let m = mem.by_str_mut(k.trim());
								let _ =
									ty.to_writer(&m.data, &mut m.vec.as_mut_slice(), &version.map);
								match k.trim() {
									"ST_ADS_TO_BC" => {
										client.do_send(AdsWriteReq {
											index_group: struct_map.st_ads_to_bc.index_group,
											index_offset: struct_map.st_ads_to_bc.index_offset,
											length: m.vec.len() as u32,
											data: m.vec.clone(),
										});
									}
									"ST_RETAIN_DATA" => {
										client.do_send(AdsWriteReq {
											index_group: struct_map.st_retain_data.index_group,
											index_offset: struct_map.st_retain_data.index_offset,
											length: m.vec.len() as u32,
											data: m.vec.clone(),
										});
									}
									_ => unreachable!(),
								}
							}
							None
						})
						.collect();
				}
			}
			WsToAdsClient::Resolve(schema) => {
				use json_diff::Schema;
				let version = &self.version;
				if let Schema::Root(v) = &schema {
					schema.as_schema(&Value::Object(
						v.iter()
							.filter_map(|f| {
								let name = match f {
									Schema::Obj(n, _) => n,
									Schema::Tag(n) => n,
									Schema::Root(_) => unreachable!(),
								};
								match version.search_index.get(&name.trim().to_string()) {
									Some(key_guard) => {
										let ty: &AdsType = &*version.map.get(&*key_guard).unwrap();
										let v = self.data.by_str(name.trim());
										match name.trim() {
											"ST_ADS_TO_BC" => {
												self.client.do_send(AdsReadReq {
													index_group: self
														.struct_map
														.st_ads_to_bc
														.index_group,
													index_offset: self
														.struct_map
														.st_ads_to_bc
														.index_offset,
													length: ty.len(),
												});
											}
											"ST_RETAIN_DATA" => {
												self.client.do_send(AdsReadReq {
													index_group: self
														.struct_map
														.st_retain_data
														.index_group,
													index_offset: self
														.struct_map
														.st_retain_data
														.index_offset,
													length: ty.len(),
												});
											}
											_ => unreachable!(),
										}
										Some((name.to_string(), v.data.clone()))
									}
									None => None,
								}
							})
							.collect(),
					));
				}
			}
			a => println!("{:?}", a),
		}
	}
}

impl<'a> Handler<AdsClientCommand> for AdsToWsMultiplexer {
	type Result = ();

	fn handle(&mut self, msg: AdsClientCommand, ctx: &mut Self::Context) {
		println!("h: {:?}", msg);
		match msg {
			AdsClientCommand::ReadRetain(len) => {
				self.client.do_send(AdsReadReq {
					index_group: self.struct_map.st_retain_data.index_group,
					index_offset: self.struct_map.st_retain_data.index_offset,
					length: len,
				});
			}
			AdsClientCommand::ReadToBc(len) => {
				self.client.do_send(AdsReadReq {
					index_group: self.struct_map.st_ads_to_bc.index_group,
					index_offset: self.struct_map.st_ads_to_bc.index_offset,
					length: len,
				});
			}
			AdsClientCommand::WriteRetain(d) => {
				self.client.do_send(AdsWriteReq {
					index_group: self.struct_map.st_retain_data.index_group,
					index_offset: self.struct_map.st_retain_data.index_offset,
					length: d.len() as u32,
					data: d,
				});
			}
			AdsClientCommand::WriteToBc(d) => {
				self.client.do_send(AdsWriteReq {
					index_group: self.struct_map.st_ads_to_bc.index_group,
					index_offset: self.struct_map.st_ads_to_bc.index_offset,
					length: d.len() as u32,
					data: d,
				});
			}
		}
	}
}
