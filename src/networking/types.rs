use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std;
#[derive(Debug, Clone)]
#[repr(C)]
pub enum AdsCommandData {
    ReadReq {
        index_group: u32,
        index_offset: u32,
        length: u32,
    },
    ReadRes {
        result: u32,
        length: u32,
        data: Vec<u8>,
    },
    WriteReq {
        index_group: u32,
        index_offset: u32,
        length: u32,
        data: Vec<u8>,
    },
    WriteRes {
        result: u32,
    },
}

#[derive(Debug, Clone)]
pub struct AmsTcpHeader {
    pub length: u32,
    pub header: AmsHeader,
}

#[derive(Debug, Clone)]
pub struct AmsHeader {
    pub target: Vec<u8>,
    pub source: Vec<u8>,
    pub command_id: u16,
    pub state_flags: u16,
    pub inv_id: u32,
    pub data: AdsCommandData,
}

impl AdsCommandData {
    pub fn from_reader<T: ReadBytesExt + std::fmt::Debug>(
        t: &mut T,
        command_id: u16,
        state_flags: u16,
    ) -> Self {
        match (command_id, state_flags) {
            (2, 4) => AdsCommandData::ReadReq {
                index_group: t.read_u32::<LittleEndian>().unwrap(),
                index_offset: t.read_u32::<LittleEndian>().unwrap(),
                length: t.read_u32::<LittleEndian>().unwrap(),
            },
            (2, 5) => {
                let result = t.read_u32::<LittleEndian>().unwrap();
                let length = t.read_u32::<LittleEndian>().unwrap();
                AdsCommandData::ReadRes {
                    result,
                    length,
                    data: {
                        let mut buf = vec![0u8; length as usize];
                        let _ = t.read_exact(&mut buf);
                        buf
                    },
                }
            }
            (3, 4) => {
                let index_group = t.read_u32::<LittleEndian>().unwrap();
                let index_offset = t.read_u32::<LittleEndian>().unwrap();
                let length = t.read_u32::<LittleEndian>().unwrap();
                AdsCommandData::WriteReq {
                    index_group,
                    index_offset,
                    length,
                    data: {
                        let mut buf = vec![0u8; length as usize];
                        let _ = t.read_exact(&mut buf);
                        buf
                    },
                }
            }
            (3, 5) => AdsCommandData::WriteRes {
                result: t.read_u32::<LittleEndian>().unwrap(),
            },
            p => panic!("unknown state_flags command_id pattern {:#?}", p),
        }
    }
    pub fn into_writer<U: WriteBytesExt + std::fmt::Debug>(self, writer: &mut U) {
        match self {
            AdsCommandData::ReadReq {
                index_group,
                index_offset,
                length,
            } => {
                let _ = writer.write_u32::<LittleEndian>(index_group);
                let _ = writer.write_u32::<LittleEndian>(index_offset);
                let _ = writer.write_u32::<LittleEndian>(length);
            }
            AdsCommandData::ReadRes {
                result,
                length,
                data,
            } => {
                let _ = writer.write_u32::<LittleEndian>(result);
                let _ = writer.write_u32::<LittleEndian>(length);
                let _ = writer.write_all(data.as_slice());
            }
            AdsCommandData::WriteReq {
                index_group,
                index_offset,
                length,
                data,
            } => {
                let _ = writer.write_u32::<LittleEndian>(index_group);
                let _ = writer.write_u32::<LittleEndian>(index_offset);
                let _ = writer.write_u32::<LittleEndian>(length);
                let _ = writer.write_all(data.as_slice());
            }
            AdsCommandData::WriteRes { result } => {
                let _ = writer.write_u32::<LittleEndian>(result);
            }
        };
    }

    pub fn gen_res(&self) -> AdsCommandData {
        match self {
            AdsCommandData::ReadReq { length, .. } => AdsCommandData::ReadRes {
                result: 0,
                length: *length,
                data: vec![0u8; *length as usize],
            },
            AdsCommandData::WriteReq { .. } => AdsCommandData::WriteRes { result: 0 },
            _ => panic!("cant create res from res"),
        }
    }

    pub fn size_of(&self) -> u32 {
        match self {
            AdsCommandData::ReadRes { length, .. } => 8 + *length,
            AdsCommandData::ReadReq { .. } => 12,
            AdsCommandData::WriteReq { length, .. } => 12 + length,
            AdsCommandData::WriteRes { .. } => 4,
        }
    }
}

impl AmsHeader {
    pub fn from_reader<T: ReadBytesExt + std::fmt::Debug>(t: &mut T) -> Self {
        let mut target = vec![0; 8];
        let mut source = vec![0; 8];
        let _ = t.read_exact(&mut target);
        let _ = t.read_exact(&mut source);
        let command_id: u16 = t.read_u16::<LittleEndian>().unwrap();
        let state_flags: u16 = t.read_u16::<LittleEndian>().unwrap();
        let _: u64 = t.read_u64::<LittleEndian>().unwrap();
        let invoke_id: u32 = t.read_u32::<LittleEndian>().unwrap();
        AmsHeader {
            target: target,
            source: source,
            command_id,
            state_flags,
            inv_id: invoke_id,
            data: AdsCommandData::from_reader(t, command_id, state_flags),
        }
    }

    pub fn into_writer<U: WriteBytesExt + std::fmt::Debug>(self, writer: &mut U) {
        let _ = writer.write_all(self.target.as_slice());
        let _ = writer.write_all(self.source.as_slice());
        let _ = writer.write_u16::<LittleEndian>(self.command_id);
        let _ = writer.write_u16::<LittleEndian>(self.state_flags);
        let mut v = Vec::new();
        self.data.into_writer(&mut v);
        let _ = writer.write_u32::<LittleEndian>(v.len() as u32);
        let _ = writer.write_u32::<LittleEndian>(0);
        let _ = writer.write_u32::<LittleEndian>(self.inv_id);
        let _ = writer.write_all(v.as_slice());
    }
    pub fn gen_res(&self, data: Option<AdsCommandData>) -> AmsHeader {
        AmsHeader {
            target: self.source.clone(),
            source: self.target.clone(),
            command_id: self.command_id,
            state_flags: 5,
            inv_id: self.inv_id,
            data: data.unwrap_or(self.data.gen_res()),
        }
    }
}

impl AmsTcpHeader {
    pub fn from_reader<T: ReadBytesExt + std::fmt::Debug>(t: &mut T) -> Self {
        let _ = t.read_u16::<LittleEndian>();
        let mut buf = [0u8; 4];
        let _ = t.read_exact(&mut buf);
        let length: u32 = (&buf[..]).read_u32::<LittleEndian>().unwrap();
        AmsTcpHeader {
            length,
            header: AmsHeader::from_reader(t),
        }
    }

    pub fn into_writer<U: WriteBytesExt + std::fmt::Debug>(self, writer: &mut U) {
        let mut buf = vec![0u8; 2];
        let _ = buf.write_u32::<LittleEndian>(self.length);
        let _ = writer.write_all(buf.as_mut_slice());
        self.header.into_writer(writer);
    }

    pub fn gen_result(&self, data: Option<AdsCommandData>) -> Self {
        if self.header.state_flags == 4 {
            let header = self.header.gen_res(data);
            AmsTcpHeader {
                length: 32 + header.data.size_of(),
                header,
            }
        } else {
            panic!("cant make result for response");
        }
    }
}
