use bus::{Bus, BusReader};
use byteorder::{LittleEndian, ReadBytesExt};
use channel::{Receiver, Sender};
use crossbeam::sync::MsQueue;
use rand::Rng;
use std::fmt;
use std::io;
use std::io::prelude::*;
use std::net::{self, TcpStream};
use std::sync::{self, Arc};
use std::thread;

use channel;
use rand;
use std::collections::HashMap;
use std::time::Duration;

mod types;

const MS: &Duration = &Duration::from_millis(10);

use self::types::{AdsCommandData, AmsHeader, AmsTcpHeader};

pub trait ToPlcConn {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]>;
}

impl ToPlcConn for (String, u16) {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        let net_id: Vec<_> = self
            .0
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

impl<'a> ToPlcConn for (&'a str, &'a str) {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        (self.0.to_string(), u16::from_str_radix(self.1, 10).unwrap()).try_into_plc_conn()
    }
}

impl<'a> ToPlcConn for &'a str {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        let r: Vec<_> = self.split(':').collect();
        if r.len() != 2 {
            None
        } else {
            (r[0], r[1]).try_into_plc_conn()
        }
    }
}

impl<'a> ToPlcConn for &'a [u8] {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        if self.len() < 8 {
            None
        } else {
            let mut d = [0u8; 8];
            let _: Vec<_> = d
                .iter_mut()
                .zip(self.iter())
                .map(|(data, origin)| *data = *origin)
                .collect();
            Some(d)
        }
    }
}
type ResultSender = Sender<AmsTcpHeader>;
type Queue = MsQueue<(Vec<u8>, u32, ResultSender)>;

#[derive(Clone)]
pub struct SimpleSocket {
    alive: Arc<sync::RwLock<bool>>,
    queue: Arc<Queue>,
    target: [u8; 8],
    source: [u8; 8],
    sender: Arc<sync::Mutex<Bus<AmsTcpHeader>>>,
}

impl fmt::Debug for SimpleSocket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "SimpleSocket {{ {:?}, {:?}, {:?}, {:?} }}",
            self.alive, self.queue, self.target, self.source
        )
    }
}

impl SimpleSocket {
    pub fn new<A: net::ToSocketAddrs + fmt::Debug, B: ToPlcConn, C: ToPlcConn>(
        a: &A,
        target: &B,
        source: &C,
    ) -> Result<Self, io::Error> {
        println!("{:?}", a);
        let mut stream = TcpStream::connect(a)?;
        stream.set_nonblocking(true)?;
        let lock = Arc::new(sync::RwLock::new(false));
        let q: Arc<Queue> = Arc::new(MsQueue::new());
        let q_thread = q.clone();
        let lock_thread = lock.clone();
        let c = Arc::new(sync::Mutex::new(Bus::new(10)));
        let thread_c = c.clone();
        thread::spawn(move || {
            let mut map = HashMap::new();
            let mut b = vec![0u8; 6];
            'outter: loop {
                if *lock_thread.read().unwrap() {
                    break 'outter;
                }
                if let Some(t) = q_thread.try_pop() {
                    while let Err(_) = stream.write_all(&t.0) {
                        thread::sleep(*MS);
                    }
                    map.insert(t.1, t.2);
                } else if stream.read_exact(&mut b).is_ok() {
                    let len: u32 = (&b[2..6]).read_u32::<LittleEndian>().unwrap();
                    let mut buf = vec![0u8; len as usize];
                    while let Err(_) = stream.read_exact(&mut buf) {
                        thread::sleep(*MS);
                    }
                    b.extend(buf);
                    let tcp = AmsTcpHeader::from_reader(&mut b.as_slice());
                    if map.contains_key(&tcp.header.inv_id) {
                        let tx = map.remove(&tcp.header.inv_id).unwrap();
                        tx.send(tcp);
                    } else {
                        let result = tcp.gen_result(None);
                        thread_c.lock().unwrap().broadcast(result.clone());
                        let mut data_v = Vec::new();
                        result.into_writer(&mut data_v);
                        while let Err(_) = stream.write_all(&data_v) {}
                    }
                    b = vec![0u8; 6];
                }
            }
            println!("closing");
        });
        Ok(SimpleSocket {
            alive: lock,
            queue: q,
            target: target
                .try_into_plc_conn()
                .expect("expected valid netid and port"),
            source: source
                .try_into_plc_conn()
                .expect("expected valid netid and port"),
            sender: c,
        })
    }

    fn get_recv(&self) -> BusReader<AmsTcpHeader> {
        self.sender.lock().unwrap().add_rx()
    }

    #[cfg_attr(feature = "cargo-clippy", allow(let_unit_value))]
    fn write(
        &self,
        idx_grp: u32,
        idx_offs: u32,
        data: Vec<u8>,
        request_counter: Option<u32>,
    ) -> Receiver<AmsTcpHeader> {
        let data_len = data.len() as u32;
        let count =
            request_counter.unwrap_or_else(|| rand::thread_rng().gen_range(0, <u32>::max_value()));
        let req = AmsTcpHeader {
            length: 32 + 12 + data_len,
            header: AmsHeader {
                target: self.target.to_vec(),
                source: self.source.to_vec(),
                command_id: 3,
                state_flags: 4,
                inv_id: count,
                data: AdsCommandData::WriteReq {
                    index_group: idx_grp,
                    index_offset: idx_offs,
                    length: data_len,
                    data,
                },
            },
        };
        let mut v = Vec::with_capacity(32 + 12 + data_len as usize + 6);
        let _ = req.into_writer(&mut v);
        let (tx, rx) = channel::bounded(1);
        self.queue.push((v, count, tx));
        rx
    }
}

/*fn main() -> Result<(), std::io::Error> {
    let s = Arc::new(SimpleSocket::new("172.16.21.123:48898", "10.15.96.64.1.1:851", "172.16.21.2.1.1:801")?);
    let s2 = s.clone();
    let mut i: u32 = 0;
    loop {
        let rx = s.write(0x4020,0, to_v(&i), None);
        let _ = rx.recv();
        let _ = rx.recv();
        i+=1;
        if i == <u32>::max_value() {
            i = 0;
        }
        thread::sleep_ms(3000);
    }
    Ok(())
}
*/
