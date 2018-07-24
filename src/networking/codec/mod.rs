use byteorder::{ByteOrder, LittleEndian};
use bytes::{BufMut, BytesMut, IntoBuf};
use std::io;
use tokio_io::codec::{Decoder, Encoder};
pub mod types;
pub use self::types::*;
use actix::Message;

#[derive(Debug, Clone)]
pub enum AdsPacket {
    WriteReq(AmsTcpHeader<types::AdsWriteReq>),
    WriteRes(AmsTcpHeader<types::AdsWriteRes>),
    ReadReq(AmsTcpHeader<types::AdsReadReq>),
    ReadRes(AmsTcpHeader<types::AdsReadRes>),
}

impl Message for AdsPacket {
    type Result = AdsPacket;
}

pub struct AdsClientCodec;

impl Decoder for AdsClientCodec {
    type Item = AdsPacket;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let src_len = src.len();
        let p = src.to_vec();
        let size = {
            if src_len < 6 {
                return Ok(None);
            }
            LittleEndian::read_u32(&p[2..])
        };

        if src_len >= size as usize + 6 {
            let c_id = LittleEndian::read_u16(&p[6 + 16..]);
            let s_flag = LittleEndian::read_u16(&p[24..]);
            let mut b = io::Cursor::new(src);
            let r = match (c_id, s_flag) {
                (3, 4) => Some(AdsPacket::WriteReq(AmsTcpHeader::from_buf(&mut b))),
                (3, 5) => Some(AdsPacket::WriteRes(AmsTcpHeader::from_buf(&mut b))),
                (2, 4) => Some(AdsPacket::ReadReq(AmsTcpHeader::from_buf(&mut b))),
                (2, 5) => Some(AdsPacket::ReadRes(AmsTcpHeader::from_buf(&mut b))),
                _ => None,
            };
            b.into_inner().clear();
            Ok(r)
        } else {
            Ok(None)
        }
    }
}

impl Encoder for AdsClientCodec {
    type Item = AdsPacket;
    type Error = io::Error;

    fn encode(&mut self, msg: AdsPacket, dst: &mut BytesMut) -> Result<(), Self::Error> {
        match msg {
            AdsPacket::ReadReq(r) => {
                dst.reserve(r.size());
                dst.put(r.into_buf());
            }
            AdsPacket::ReadRes(r) => {
                dst.reserve(r.size());
                dst.put(r.into_buf());
            }
            AdsPacket::WriteReq(r) => {
                dst.reserve(r.size());
                dst.put(r.into_buf());
            }
            AdsPacket::WriteRes(r) => {
                dst.reserve(r.size());
                dst.put(r.into_buf());
            }
        }
        Ok(())
    }
}
