use actix::Message;
use byteorder::{LittleEndian, WriteBytesExt};
use bytes::{Buf, IntoBuf};
use std::io;

pub trait AdsCommand: IntoBuf + Clone {
    type Result: AdsCommand;
    fn size(&self) -> usize;
    fn from_buf(src: &mut impl Buf) -> Self;
    fn gen_res(&self) -> Self::Result;
}

#[derive(Debug, Clone)]
pub struct AdsReadReq {
    pub index_group: u32,
    pub index_offset: u32,
    pub length: u32,
}

impl Message for AdsReadReq {
    type Result = Result<AdsReadRes, ()>;
}

#[derive(Debug, Clone)]
pub struct AdsReadRes {
    pub result: u32,
    pub length: u32,
    pub data: Vec<u8>,
}

impl Message for AdsReadRes {
    type Result = ();
}

/*impl<A, M> MessageResponse<A, M> for Box<Future<Error = (), Item = AdsReadRes> + 'static>
where
    A: Actor,
    M: Message<Result = AdsReadRes>,
{
    fn handle<R: ResponseChannel<M>>(self, ctx: &mut A::Context, tx: Option<R>) {
        if let Some(tx) = tx {
            tx.send(self);
        }
    }
}*/

#[derive(Debug, Clone)]
pub struct AdsWriteReq {
    pub index_group: u32,
    pub index_offset: u32,
    pub length: u32,
    pub data: Vec<u8>,
}

impl Message for AdsWriteReq {
    type Result = ();
}

#[derive(Debug, Clone)]
pub struct AdsWriteRes {
    result: u32,
}

impl Message for AdsWriteRes {
    type Result = ();
}

impl AdsCommand for AdsWriteRes {
    type Result = AdsWriteReq;
    fn size(&self) -> usize {
        4
    }

    fn from_buf(src: &mut impl Buf) -> Self {
        AdsWriteRes {
            result: src.get_u32_le(),
        }
    }

    fn gen_res(&self) -> Self::Result {
        unreachable!()
    }
}

impl IntoBuf for AdsWriteRes {
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(4);
        let _ = v.write_u32::<LittleEndian>(self.result);
        io::Cursor::new(v)
    }
}

impl AdsCommand for AdsWriteReq {
    type Result = AdsWriteRes;
    fn size(&self) -> usize {
        self.length as usize + 12
    }

    fn from_buf(r: &mut impl Buf) -> Self {
        let index_group = r.get_u32_le();
        let index_offset = r.get_u32_le();
        let length = r.get_u32_le();
        AdsWriteReq {
            index_group,
            index_offset,
            length,
            data: get_vec(r, length as usize),
        }
    }

    fn gen_res(&self) -> Self::Result {
        AdsWriteRes { result: 0 }
    }
}

impl IntoBuf for AdsWriteReq {
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(12 + self.length as usize);
        let _ = v.write_u32::<LittleEndian>(self.index_group);
        let _ = v.write_u32::<LittleEndian>(self.index_offset);
        let _ = v.write_u32::<LittleEndian>(self.length);
        v.extend(self.data);
        io::Cursor::new(v)
    }
}

impl AdsCommand for AdsReadReq {
    type Result = AdsReadRes;
    fn size(&self) -> usize {
        12
    }

    fn from_buf(r: &mut impl Buf) -> Self {
        AdsReadReq {
            index_group: r.get_u32_le(),
            index_offset: r.get_u32_le(),
            length: r.get_u32_le(),
        }
    }

    fn gen_res(&self) -> Self::Result {
        AdsReadRes {
            result: 1793,
            length: 0,
            data: vec![],
        }
    }
}

impl IntoBuf for AdsReadReq {
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(12);
        let _ = v.write_u32::<LittleEndian>(self.index_group);
        let _ = v.write_u32::<LittleEndian>(self.index_offset);
        let _ = v.write_u32::<LittleEndian>(self.length);
        io::Cursor::new(v)
    }
}

impl AdsCommand for AdsReadRes {
    type Result = AdsReadReq;
    fn size(&self) -> usize {
        self.length as usize + 8
    }

    fn from_buf(r: &mut impl Buf) -> Self {
        let result = r.get_u32_le();
        let length = r.get_u32_le();
        AdsReadRes {
            result,
            length,
            data: get_vec(r, length as usize),
        }
    }

    fn gen_res(&self) -> Self::Result {
        unreachable!()
    }
}

impl IntoBuf for AdsReadRes {
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(12 + self.length as usize);
        let _ = v.write_u32::<LittleEndian>(self.result);
        let _ = v.write_u32::<LittleEndian>(self.length);
        v.extend(self.data);
        io::Cursor::new(v)
    }
}

#[derive(Debug, Clone)]
pub struct AmsTcpHeader<T>
where
    T: AdsCommand,
{
    pub length: u32,
    pub header: AmsHeader<T>,
}

#[derive(Debug, Clone)]
pub struct AmsHeader<T>
where
    T: AdsCommand,
{
    pub target: [u8; 8],
    pub source: [u8; 8],
    pub command_id: u16,
    pub state_flags: u16,
    pub inv_id: u32,
    pub data: T,
}

fn get_vec(s: &mut impl Buf, size: usize) -> Vec<u8> {
    (0..size).map(|_| s.get_u8()).collect()
}

fn get_ams_conn(s: &mut impl Buf) -> [u8; 8] {
    let mut d = [0u8; 8];
    for i in &mut d.iter_mut() {
        *i = s.get_u8();
    }
    d
}

impl<T> AdsCommand for AmsHeader<T>
where
    T: AdsCommand,
{
    type Result = AmsHeader<<T as AdsCommand>::Result>;
    fn size(&self) -> usize {
        32 + self.data.size()
    }

    fn from_buf(r: &mut impl Buf) -> Self {
        AmsHeader {
            target: get_ams_conn(r),
            source: get_ams_conn(r),
            command_id: r.get_u16_le(),
            state_flags: r.get_u16_le(),
            inv_id: {
                r.get_u32_le();
                r.get_u32_le();
                r.get_u32_le()
            },
            data: T::from_buf(r),
        }
    }

    fn gen_res(&self) -> Self::Result {
        let data_res = self.data.gen_res();
        AmsHeader {
            target: self.source,
            source: self.target,
            command_id: self.command_id,
            state_flags: 5, //gen result from request isnt possible
            inv_id: self.inv_id,
            data: data_res,
        }
    }
}

impl<T> IntoBuf for AmsHeader<T>
where
    T: AdsCommand,
{
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(16 + self.data.size());
        let _ = v.write_u16::<LittleEndian>(self.command_id);
        let _ = v.write_u16::<LittleEndian>(self.state_flags);
        let _ = v.write_u32::<LittleEndian>(self.data.size() as u32);
        let _ = v.write_u32::<LittleEndian>(0);
        let _ = v.write_u32::<LittleEndian>(self.inv_id);
        let h: Vec<_> = self.data.into_buf().collect();
        v.extend(h);
        let mut data = Vec::with_capacity(16 + v.len());
        data.extend_from_slice(&self.target[..]);
        data.extend_from_slice(&self.source[..]);
        data.extend(v);
        io::Cursor::new(data)
    }
}

impl<T> AdsCommand for AmsTcpHeader<T>
where
    T: AdsCommand,
{
    type Result = AmsTcpHeader<<T as AdsCommand>::Result>;
    fn size(&self) -> usize {
        6 + self.header.size()
    }

    fn from_buf(src: &mut impl Buf) -> Self {
        src.advance(2);
        AmsTcpHeader {
            length: src.get_u32_le(),
            header: AmsHeader::from_buf(src),
        }
    }

    fn gen_res(&self) -> Self::Result {
        let h = self.header.gen_res();
        AmsTcpHeader {
            length: h.size() as u32,
            header: h,
        }
    }
}

impl<T> IntoBuf for AmsTcpHeader<T>
where
    T: AdsCommand,
{
    type Buf = io::Cursor<Vec<u8>>;

    fn into_buf(self) -> Self::Buf {
        let mut v = Vec::with_capacity(6);
        let h = self.header.into_buf();
        let _ = v.write_u16::<LittleEndian>(0);
        let _ = v.write_u32::<LittleEndian>(h.get_ref().len() as u32);
        v.extend(h.into_inner());
        io::Cursor::new(v)
    }
}

/*impl<T> Message for AmsTcpHeader<T>
where
    T: AdsCommand + 'static,
{
    type Result = AmsTcpHeader<<T as AdsCommand>::Result>;

}*/
