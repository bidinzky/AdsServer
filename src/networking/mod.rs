use rand::Rng;
use std::io;
use std::sync::{Arc, self};
use std::net::{TcpStream, self};
use std::io::prelude::*;
use crossbeam::sync::MsQueue;
use channel::{Sender, Receiver};
use std::thread;
use byteorder::{ReadBytesExt, LittleEndian};

use std::time::Duration;
use channel;
use rand;
use std::collections::HashMap;

mod types;

const MS: &Duration = &Duration::from_millis(1);

use self::types::{
    AmsTcpHeader,
    AmsHeader,
    AdsCommandData
};


trait ToPlcConn {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]>;
}

impl<'a> ToPlcConn for &'a str {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        let r: Vec<_> = self.split(":").collect();
        if r.len() != 2 {
            None
        }else{
            let net_id: Vec<_> = r[0].split(".").map(|x| u8::from_str_radix(x, 10).unwrap()).collect();
            let port = u16::from_str_radix(r[1], 10).unwrap();
            let mut d = [0u8; 8];
            for i in 0..6 {
                d[i] = net_id[i];
            }
            d[7] = ((port >> 8) & 0xff) as u8;
            d[6] = (port & 0xff) as u8;
            Some(d)
        }
    }
}

impl<'a> ToPlcConn for &'a [u8] {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        if self.len() < 8 {
            None
        }else{
            let mut d = [0u8;8];
            let _: Vec<_> = d
                .iter_mut()
                .zip(self.iter())
                .map(|(data, origin)| *data = *origin)
                .collect();
            Some(d)
        }
    }
}

struct SimpleSocket {
    alive: Arc<sync::RwLock<bool>>,
    queue: Arc<MsQueue<(Vec<u8>, u32, Sender<AmsTcpHeader>)>>,
    target: [u8; 8],
    source: [u8; 8]
}

impl SimpleSocket {
    fn new<A: net::ToSocketAddrs, B: ToPlcConn, C: ToPlcConn>(a: A, target: B, source: C) -> Result<Self, io::Error> {
        let mut stream = TcpStream::connect(a)?;
        stream.set_nonblocking(true)?;
        let lock = Arc::new(sync::RwLock::new(false));
        let q: Arc<MsQueue<(Vec<u8>, u32, Sender<AmsTcpHeader>)>> = Arc::new(MsQueue::new());
        let q_thread = q.clone();
        let lock_thread = lock.clone();
        thread::spawn(move || {
            let mut map = HashMap::new();
            let mut b = vec![0u8;6];
            'outter: loop {
                if *lock_thread.read().unwrap() {
                    break 'outter;
                }
                if let Some(mut t) = q_thread.try_pop() {
                    while let Err(_) = stream.write_all(&mut t.0) {
                        thread::sleep(*MS);
                    }
                    map.insert(t.1, t.2);
                }else {
                    match stream.read_exact(&mut b) {
                        Ok(_) => {
                            let len: u32 = (&b[2..6]).read_u32::<LittleEndian>().unwrap();;
                            let mut buf = vec![0u8; len as usize];
                            while let Err(_) = stream.read_exact(&mut buf) {
                                thread::sleep(*MS);
                            }
                            b.extend(buf);
                            let tcp = AmsTcpHeader::from_reader(&mut b.as_slice());
                            if map.contains_key(&tcp.header.inv_id) {
                                let tx = map.remove(&tcp.header.inv_id).unwrap();
                                tx.send(tcp);
                            }else{
                                let r = tcp.gen_result(None);
                                let mut v = Vec::new();
                                r.into_writer(&mut v);
                                while let Err(_) = stream.write_all(&mut v) {}
                            }
                            b = vec![0u8;6];
                        },
                        _ => {}
                    }
                }
            }
            println!("closing");
        });
        Ok(SimpleSocket {
            alive: lock,
            queue: q,
            target: target.try_into_plc_conn().expect("expected valid netid and port"),
            source: source.try_into_plc_conn().expect("expected valid netid and port")
        })
    }

    fn write(&self, idx_grp: u32, idx_offs: u32, data: Vec<u8>, request_counter: Option<u32>) -> Receiver<AmsTcpHeader> {
        let data_len = data.len() as u32;
        let count = request_counter.unwrap_or(rand::thread_rng().gen_range(0, <u32>::max_value()));
        let req = AmsTcpHeader {
            length: 32+12+data_len,
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
                    data: data
                }
            }
        };
        let mut v = Vec::with_capacity(32+12+data_len as usize+ 6);
        let _ = req.into_writer(&mut v);
        let (tx,rx) = channel::bounded(1);
        self.queue.push((v, count, tx));
        return rx;
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