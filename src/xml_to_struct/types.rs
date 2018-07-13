use super::helper::{number_from_value, read_ads_number, type_from_value, write_ads_number};
use super::Value;
use byteorder::{ReadBytesExt, WriteBytesExt};
use chashmap::CHashMap;
use std::collections::HashMap;
use std::io;

#[derive(Debug)]
pub struct AdsVersion {
    pub map: CHashMap<String, AdsType>,
    pub symbols: CHashMap<String, Symbol>,
    pub search_index: CHashMap<String, String>,
}

#[derive(Debug)]
pub struct Symbol {
    pub index_group: u32,
    pub index_offset: u32,
    pub name: Name,
    pub ty: AdsPlcType,
}

#[derive(Debug)]
pub struct Name {
    pub text: String,
    pub decoration: Option<String>,
}

#[derive(Debug, Clone)]
pub enum AdsPlcType {
    Bool,
    SInt(Option<SubRange>),
    USInt(Option<SubRange>),
    Int(Option<SubRange>),
    UInt(Option<SubRange>),
    DInt(Option<SubRange>),
    UDInt(Option<SubRange>),
    Real,
    LReal,
    String(usize),
    Time,
    TOD,
    Date,
    DT,
    Other { name: String, reference: String },
}

#[derive(Debug)]
pub struct AdsStructProperties {
    pub name: String,
    pub ty: AdsPlcType,
}

#[derive(Debug, Copy, Clone)]
pub struct SubRange {
    pub min: i64,
    pub max: i64,
}

#[derive(Debug)]
pub enum AdsType {
    Enum {
        name: String,
        keys: HashMap<i16, String>,
    },
    Struct {
        name: String,
        properties: Vec<AdsStructProperties>,
    },
    Array {
        bounds: Vec<(usize, usize)>,
        ty: AdsPlcType,
    },
    Primitive(AdsPlcType),
}

impl AdsPlcType {
    pub fn to_writer<W: WriteBytesExt>(
        &self,
        data: &Value,
        w: &mut W,
        map: &HashMap<String, AdsType>,
    ) -> Result<(), io::Error> {
        match self {
            AdsPlcType::Bool => w.write_u8(data.as_bool().expect("no bool") as u8),
            AdsPlcType::SInt(ref s) => {
                write_ads_number::<i8, W>(data.as_i64().expect("no sint") as i8, w, s)
            }
            AdsPlcType::Int(ref s) => {
                write_ads_number::<i16, W>(data.as_i64().expect("no int") as i16, w, s)
            }
            AdsPlcType::DInt(ref s) => {
                write_ads_number::<i32, W>(data.as_i64().expect("no dint") as i32, w, s)
            }
            AdsPlcType::USInt(ref s) => {
                write_ads_number::<u8, W>(data.as_u64().expect("no usint") as u8, w, s)
            }
            AdsPlcType::UInt(ref s) => {
                write_ads_number::<u16, W>(data.as_u64().expect("no uint") as u16, w, s)
            }
            AdsPlcType::UDInt(ref s) => {
                write_ads_number::<u32, W>(data.as_u64().expect("no udint") as u32, w, s)
            }
            AdsPlcType::Real => {
                write_ads_number::<f32, W>(data.as_f64().expect("no real") as f32, w, &None)
            }
            AdsPlcType::LReal => {
                write_ads_number::<f64, W>(data.as_f64().expect("no lreal") as f64, w, &None)
            }
            AdsPlcType::Date => {
                write_ads_number::<u32, W>(data.as_u64().expect("no date") as u32, w, &None)
            }
            AdsPlcType::DT => {
                write_ads_number::<u32, W>(data.as_u64().expect("no dt") as u32, w, &None)
            }
            AdsPlcType::TOD => {
                write_ads_number::<u32, W>(data.as_u64().expect("no tod") as u32, w, &None)
            }
            AdsPlcType::Time => {
                write_ads_number::<u32, W>(data.as_u64().expect("no time") as u32, w, &None)
            }
            AdsPlcType::String(ref len) => {
                let strs = data.as_str().expect("no str").as_bytes();
                w.write_all(&strs[..=*len])
            }
            AdsPlcType::Other { ref reference, .. } => map[reference].to_writer(data, w, map),
        }
    }
    pub fn as_data_struct<R: ReadBytesExt>(
        &self,
        r: &mut R,
        map: &HashMap<String, AdsType>,
    ) -> Value {
        match self {
            AdsPlcType::Bool => (r.read_u8().unwrap() >= 1).into(),
            AdsPlcType::SInt(ref s) => read_ads_number::<i8, R>(r, s),
            AdsPlcType::Int(ref s) => read_ads_number::<i16, R>(r, s),
            AdsPlcType::DInt(ref s) => read_ads_number::<i32, R>(r, s),
            AdsPlcType::USInt(ref s) => read_ads_number::<u8, R>(r, s),
            AdsPlcType::UInt(ref s) => read_ads_number::<u16, R>(r, s),
            AdsPlcType::UDInt(ref s) => read_ads_number::<u32, R>(r, s),
            AdsPlcType::Real => read_ads_number::<f32, R>(r, &None),
            AdsPlcType::LReal => read_ads_number::<f64, R>(r, &None),
            AdsPlcType::Date => read_ads_number::<u32, R>(r, &None),
            AdsPlcType::DT => read_ads_number::<u32, R>(r, &None),
            AdsPlcType::TOD => read_ads_number::<u32, R>(r, &None),
            AdsPlcType::Time => read_ads_number::<u32, R>(r, &None),
            AdsPlcType::String(ref len) => {
                let mut b = vec![0u8; *len];
                let _ = r.read_exact(&mut b);
                Value::String(String::from_utf8(b).unwrap())
            }
            AdsPlcType::Other { ref reference, .. } => map[reference].as_data_struct(r, map),
        }
    }
}

impl AdsType {
    pub fn to_writer<W: WriteBytesExt>(
        &self,
        data: &Value,
        w: &mut W,
        map: &HashMap<String, AdsType>,
    ) -> Result<(), io::Error> {
        match self {
            AdsType::Enum { keys, .. } => {
                let mut i = data.as_i64().expect("no enum") as i16;
                if !keys.contains_key(&i) {
                    let mut ik = keys.iter();
                    let (first, _) = ik.next().unwrap();
                    let (last, _) = ik.last().unwrap();
                    if &i < first {
                        i = *first;
                    } else if &i > last {
                        i = *last;
                    }
                }
                write_ads_number(i, w, &None)
            }
            AdsType::Struct { properties, .. } => {
                properties.iter().fold(Ok(()), |acc, p| match acc {
                    Ok(_) => p.ty.to_writer(&data[&p.name], w, map),
                    _ => acc,
                })
            }
            AdsType::Array { ty, bounds, .. } => {
                let l = bounds.iter().fold(1, |acc, v| {
                    if v.1 > v.0 {
                        acc * (v.1 - v.0)
                    } else {
                        acc * (v.0 - v.1)
                    }
                });
                (0..l).map(|i| ty.to_writer(&data[i], w, map)).collect()
            }
            AdsType::Primitive(ref ty) => ty.to_writer(data, w, map),
        }
    }

    pub fn as_data_struct<R: ReadBytesExt>(
        &self,
        r: &mut R,
        map: &HashMap<String, AdsType>,
    ) -> Value {
        match self {
            AdsType::Enum { .. } => read_ads_number::<i16, R>(r, &None),
            AdsType::Struct { properties, .. } => Value::Object(
                properties
                    .iter()
                    .map(|p| (p.name.to_string(), p.ty.as_data_struct(r, map)))
                    .collect(),
            ),
            AdsType::Array {
                ref ty, ref bounds, ..
            } => {
                let l = bounds.iter().fold(1, |acc, v| {
                    if v.1 > v.0 {
                        acc * (v.1 - v.0)
                    } else {
                        acc * (v.0 - v.1)
                    }
                });
                (0..l).map(|_| ty.as_data_struct(r, map)).collect()
            }
            AdsType::Primitive(ref ty) => ty.as_data_struct(r, map),
        }
    }

    pub fn from_value<'a>(r: &'a Value) -> Option<AdsType> {
        let name: Name = (&r["Name"]).into();
        match r {
            Value::Object(ref obj) => match obj {
                obj if obj.contains_key("EnumInfo") => Some(AdsType::Enum {
                    name: name.text.to_string(),
                    keys: obj["EnumInfo"]
                        .as_array()
                        .expect("no array")
                        .iter()
                        .map(|f| {
                            let n = number_from_value(&f["Enum"]);
                            let s = f["Text"].as_str().unwrap().to_string();
                            (n, s)
                        })
                        .collect(),
                }),
                obj if obj.contains_key("SubItem") => Some(AdsType::Struct {
                    name: name.text.to_string(),
                    properties: {
                        let sub_items = match obj.get("SubItem") {
                            Some(Value::Array(ref a)) => a.clone(),
                            Some(Value::Object(ref o)) => {
                                let t: Value = (o.clone()).into();
                                vec![t]
                            }
                            _ => unreachable!(),
                        };
                        sub_items
                            .iter()
                            .map(|f| {
                                let n: Name = (&f["Name"]).into();
                                let ty = &f["Type"];
                                AdsStructProperties {
                                    name: n.text.to_string(),
                                    ty: type_from_value(&ty, None),
                                }
                            })
                            .collect()
                    },
                }),
                obj if obj.contains_key("ArrayInfo") => {
                    let mut array_info = match obj.get("ArrayInfo") {
                        Some(Value::Object(ref o)) => {
                            let t: Value = (o.clone()).into();
                            vec![t]
                        }
                        Some(Value::Array(ref a)) => a.clone(),
                        _ => unreachable!(),
                    };
                    Some(AdsType::Array {
                        bounds: array_info
                            .iter()
                            .map(|f| match f {
                                Value::Object(ref o) => (
                                    o["Elements"].as_f64().unwrap() as usize,
                                    o["LBound"].as_f64().unwrap() as usize,
                                ),
                                _ => unreachable!(),
                            })
                            .collect(),
                        ty: type_from_value(&obj["Type"], None),
                    })
                }
                obj if obj.contains_key("Type") => {
                    let sri = if let Some(r) = obj.get("SubRangeInfo") {
                        let max = r["MaxInclusive"].as_f64().unwrap() as i64;
                        let min = r["MinInclusive"].as_f64().unwrap() as i64;
                        Some(SubRange { max, min })
                    } else {
                        None
                    };
                    Some(AdsType::Primitive(type_from_value(
                        obj.get("Type").unwrap(),
                        sri,
                    )))
                }
                _ => None,
            },
            _ => None,
        }
    }
}

impl<'a> From<&'a Value> for Name {
    fn from(a: &'a Value) -> Self {
        match a {
            Value::String(r) => Name {
                text: r.clone(),
                decoration: None,
            },
            Value::Object(ref r) => Name {
                text: r["#text"].as_str().unwrap().to_string(),
                decoration: {
                    if let Some(d) = r.get("@Decoration") {
                        Some(match d {
                            Value::String(ref s) => s.to_string(),
                            Value::Number(ref n) => n.to_string(),
                            _ => unreachable!(),
                        })
                    } else {
                        None
                    }
                },
            },
            _ => unreachable!(),
        }
    }
}

impl From<(String, Option<String>, Option<SubRange>)> for AdsPlcType {
    fn from(s: (String, Option<String>, Option<SubRange>)) -> Self {
        match s.0.as_ref() {
            "BOOL" => AdsPlcType::Bool,
            "BYTE" => AdsPlcType::USInt(s.2),
            "WORD" => AdsPlcType::UInt(s.2),
            "DWORD" => AdsPlcType::UDInt(s.2),
            "SINT" => AdsPlcType::SInt(s.2),
            "USINT" => AdsPlcType::USInt(s.2),
            "INT" => AdsPlcType::Int(s.2),
            "UINT" => AdsPlcType::UInt(s.2),
            "DINT" => AdsPlcType::DInt(s.2),
            "UDINT" => AdsPlcType::UDInt(s.2),
            "REAL" => AdsPlcType::Real,
            "LREAL" => AdsPlcType::LReal,
            "TIME" => AdsPlcType::Time,
            "TIME_OF_DAY" => AdsPlcType::TOD,
            "TOD" => AdsPlcType::TOD,
            "DATE" => AdsPlcType::Date,
            "DATE_AND_TIME" => AdsPlcType::DT,
            "DT" => AdsPlcType::DT,
            st => {
                if st.contains("STRING") && !st.contains("ARRAY") {
                    let mut ost = st.replace("STRING(", "");
                    ost = ost.replace(")", "");
                    AdsPlcType::String(ost.parse().unwrap())
                } else {
                    AdsPlcType::Other {
                        name: st.to_string(),
                        reference: s.1.unwrap(),
                    }
                }
            }
        }
    }
}
