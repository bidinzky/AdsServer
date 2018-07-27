use actix::fut::{wrap_future, ActorFuture};
use actix::prelude::*;
use byteorder::{ByteOrder, LittleEndian};
use futures::{future, Future};
use json_diff::{merge, merge_schemas, merge_values, Schema};
use networking::{AdsReadReq, AdsReadRes, AdsWriteReq, Client, WsMultiplexerRegister};
use serde_json::{self, Value, to_string};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use types::Symbol;
use types::{AdsType, AdsVersion};
use ws::{Ws, WsToAdsClient, AdsToWsClient};

struct HeartBeat;

impl Message for HeartBeat {
    type Result = ();
}

#[derive(Debug)]
#[allow(non_snake_case)]
pub struct AdsMemory {
    pub data: Value,
    pub ST_ADS_TO_BC: Vec<u8>,
    pub ST_ADS_FROM_BC: Vec<u8>,
    pub ST_RETAIN_DATA: Vec<u8>,
}

impl AdsMemory {
    pub fn by_str_mut(&mut self, s: &str) -> &mut [u8] {
        match s.trim() {
            "ST_ADS_TO_BC" => &mut self.ST_ADS_TO_BC,
            "ST_ADS_FROM_BC" => &mut self.ST_ADS_FROM_BC,
            "ST_RETAIN_DATA" => &mut self.ST_RETAIN_DATA,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdsStructMap {
    pub st_ads_to_bc: Symbol,
    pub st_retain_data: Symbol,
}

impl AdsStructMap {
    pub fn by_str(&self, s: &str) -> &Symbol {
        match s.trim() {
            "ST_ADS_TO_BC" => &self.st_ads_to_bc,
            "ST_RETAIN_DATA" => &self.st_retain_data,
            _ => unreachable!(),
        }
    }
}

pub struct AdsToWsMultiplexer {
    pub subscription_map: HashMap<Addr<Ws>, Schema>,
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
            subscription_map: HashMap::new(),
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
        let f = self
            .client
            .send(WsMultiplexerRegister::Register(ctx.address()))
            .map_err(|_| eprintln!("unknowen error"));
        ctx.spawn(wrap_future(f).map(|i, _, ctx: &mut Context<Self>| {
            if let Some(rx) = i {
                ctx.add_stream(rx);
            }
        }));
        ctx.notify(HeartBeat);
    }
    fn stopped(&mut self, _: &mut Self::Context) {
        self.client.do_send(WsMultiplexerRegister::Unregister);
        println!("ads_client_mutliplexer stopped");
    }
}

impl StreamHandler<AdsWriteReq, ()> for AdsToWsMultiplexer {
    fn handle(&mut self, item: AdsWriteReq, _: &mut Self::Context) {
        // SLAVE
        if let Some(key) = self.version.search_index.get(&"ST_ADS_FROM_BC".to_string()) {
            self.data.ST_ADS_FROM_BC[(item.index_offset as usize)..item.data.len()]
                .clone_from_slice(&item.data[(item.index_offset as usize)..]);
            let ty: &AdsType = &self.version.map.get(&*key).unwrap();
            let new_data =
                ty.as_data_struct(&mut self.data.ST_ADS_FROM_BC.as_slice(), &self.version.map);
			handle_subscriptions(&self.subscription_map, &self.data.data, &new_data);
			self.data.data["ST_ADS_FROM_BC"] = new_data;
        }
    }
}

impl Handler<HeartBeat> for AdsToWsMultiplexer {
    type Result = ();

    fn handle(&mut self, _: HeartBeat, ctx: &mut Self::Context) -> Self::Result {
        let c = self.count;
        self.count += 1;
        LittleEndian::write_u32(&mut self.data.ST_ADS_TO_BC[16..20], c);
        if let Some(key) = self.version.search_index.get(&"ST_ADS_TO_BC".to_string()) {
            let ty: &AdsType = &self.version.map.get(&*key).unwrap();
            let new_data =
                ty.as_data_struct(&mut self.data.ST_ADS_TO_BC.as_slice(), &self.version.map);
            handle_subscriptions(&self.subscription_map, &self.data.data["ST_ADS_TO_BC"], &new_data);
			self.data.data["ST_ADS_TO_BC"] = new_data;
        }
        let wr = AdsWriteReq {
            index_group: self.struct_map.st_ads_to_bc.index_group,
            index_offset: self.struct_map.st_ads_to_bc.index_offset + 16,
            length: 4,
            data: self.data.ST_ADS_TO_BC[16..20].to_vec(),
        };

        self.client.do_send(wr);
        ctx.notify_later(HeartBeat, Duration::new(5, 0));
        ()
    }
}

impl Handler<WsToAdsClient> for AdsToWsMultiplexer {
    type Result = Box<ActorFuture<Item = String, Error = (), Actor = Self>>;
    fn handle(&mut self, msg: WsToAdsClient, _: &mut Self::Context) -> Self::Result {
        match msg {
            WsToAdsClient::Register(m) => {
                if !self.ws_clients.contains(&m) {
                    self.ws_clients.push(m);
                }
                Box::new(wrap_future(future::err(())))
            }
            WsToAdsClient::Unregister(m) => {
                if let Some(i) = self.ws_clients.iter().position(|x| *x == m) {
                    self.ws_clients.remove(i);
                }
                Box::new(wrap_future(future::err(())))
            }
            WsToAdsClient::Mutation(mutation) => {
                let version = &self.version;
                let mem = &mut self.data;
                let client = &self.client;
                let struct_map = &self.struct_map;
				let subscription_map = &self.subscription_map;
                if let Value::Object(obj) = mutation {
                    let _: Vec<()> = obj
                        .into_iter()
                        .filter_map(|param: (String, Value)| {
                            let (k, data) = param;
                            if let Some(key_guard) = version.search_index.get(&k.trim().to_string())
                            {
                                let ty: &AdsType = &version.map.get(&*key_guard).unwrap();
                                let data = merge(mem.data.get(k.trim()).unwrap().clone(), data);
                                {
                                    let mut m = mem.by_str_mut(k.trim());
                                    let _ = ty.to_writer(&data, &mut m, &version.map);
                                }
								handle_subscriptions(subscription_map, &mem.data[k.trim()], &data);
                                mem.data[k.trim()] = data;
                                let sm = struct_map.by_str(k.trim());
                                let m = mem.by_str_mut(k.trim());
                                client.do_send(AdsWriteReq {
                                    index_group: sm.index_group,
                                    index_offset: sm.index_offset,
                                    length: m.len() as u32,
                                    data: m.to_vec(),
                                });
                            }
                            None
                        })
                        .collect();
                }
                Box::new(wrap_future(future::err(())))
            }
            WsToAdsClient::Resolve(schema) => {
                let client = self.client.clone();
                let version = self.version.clone();
                let sm = self.struct_map.clone();
                if let Schema::Root(v) = schema {
                    Box::new(
                        wrap_future(
                            future::join_all(v.into_iter().map(move |schema_value| {
                                let name = get_name(&schema_value);
                                (
                                    future::ok(schema_value),
                                    handle_request(
                                        &client.clone(),
                                        &version.clone(),
                                        sm.by_str(&name),
                                        &name,
                                    ),
                                )
                            })).map_err(|_| println!("error {} {}", file!(), line!())),
                        ).map(|item, actor, _| {
                            serde_json::to_string(&merge_values(
                                item.into_iter()
                                    .map(|(schema_value, item)| {
                                        let name = get_name(&schema_value);
                                        handle_future(&item, actor, &schema_value, &name)
                                    })
                                    .collect(),
                            )).unwrap()
                        }),
                    )
                } else {
                    unreachable!()
                }
            }
            WsToAdsClient::Subscription(mut s, a) => {
                let v = self.subscription_map.entry(a).or_insert(s.clone());
                if v != &mut s {
                    *v = merge_schemas(v.clone(), s)
                }
                Box::new(wrap_future(future::err(())))
            }
        }
    }
}

fn get_name(s: &Schema) -> String {
    match s {
        Schema::Obj(n, _) => n.clone(),
        Schema::Tag(n) => n.clone(),
        Schema::Root(_) => unreachable!(),
    }
}
fn handle_request(
    client: &Addr<Client>,
    version: &AdsVersion,
    symbol: &Symbol,
    name: &str,
) -> impl Future<Item = AdsReadRes, Error = ()> {
    if let Some(key_guard) = version.search_index.get(&name.to_string()) {
        let ty: &AdsType = &version.map.get(&*key_guard).unwrap();
        client
            .send(AdsReadReq {
                index_group: symbol.index_group,
                index_offset: symbol.index_offset,
                length: ty.len(),
            })
            .map_err(|_| eprintln!("error"))
            .map(|f| if let Ok(f) = f { f } else { unreachable!() })
    } else {
        unreachable!()
    }
}
fn handle_future(
    item: &AdsReadRes,
    actor: &mut AdsToWsMultiplexer,
    schema: &Schema,
    name: &str,
) -> Value {
    let ndata = if let Some(key) = actor.version.search_index.get(&name.to_string()) {
        let data: &mut [u8] = actor.data.by_str_mut(name);
        let ty: &AdsType = &actor.version.map.get(&*key).unwrap();
        data[..item.data.len()].clone_from_slice(&item.data[..]);
        ty.as_data_struct(&mut &data[..], &actor.version.map)
    } else {
        unreachable!()
    };    
    handle_subscriptions(&actor.subscription_map,&actor.data.data[name], &ndata);
	actor.data.data[name] = ndata;
    schema.as_schema(&actor.data.data).unwrap()
}
fn handle_subscriptions(subscription_map: &HashMap<Addr<Ws>, Schema>,old_data: &Value, new_data: &Value) {
    for (c, s) in subscription_map {
        match s {
            Schema::Root(v) => for value in v {
                use std::fs::write;
                println!("write");
                let sv1 = value.as_schema(new_data).unwrap();
				let sv2 = value.as_schema(old_data).unwrap();
                println!("{:?} {:?}", sv1, sv2);
				if sv1 != sv2 {
                    if is_empty(&sv1) {
                        c.do_send(AdsToWsClient(to_string(&sv1).unwrap()));
                    }
				}
            },
            _ => {}
        }
    }
}

fn is_empty(v: &Value) -> bool {
    match v {
        Value::Object(m) => {
            if m.is_empty() {
                true
            }else{
                let mut acc = false;
                for (_,v) in m {
                    acc = acc || is_empty(v);
                    if !acc {
                        break;
                    }
                }
                acc
            }
        },
        Value::Array(v) => v.is_empty(),
        _ => false
    }
}