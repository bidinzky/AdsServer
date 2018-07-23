use super::{AdsPlcType, SubRange};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_traits::{Bounded, FromPrimitive, ToPrimitive};
use serde_json::Value;
use std;
use std::str::FromStr;

pub fn number_from_value<T: ToPrimitive + FromPrimitive + FromStr>(d: &Value) -> T
where
    <T as FromStr>::Err: std::fmt::Debug,
{
    match d {
        Value::String(ref s) => s.parse().unwrap(),
        Value::Number(ref n) => {
            if n.is_f64() {
                T::from_f64(n.as_f64().unwrap()).unwrap()
            } else if n.is_u64() {
                T::from_u64(n.as_u64().unwrap()).unwrap()
            } else {
                T::from_i64(n.as_i64().unwrap()).unwrap()
            }
        }
        _ => unreachable!(),
    }
}

pub fn type_from_value(d: &Value, r: Option<SubRange>) -> AdsPlcType {
    (match d {
        Value::String(ref s) => (s.to_string(), None, r),
        Value::Object(ref ty) => {
            let s = ty["#text"].as_str().unwrap().to_string();
            let d = match ty.get("@Decoration") {
                Some(ref d) => {
                    let s = match d {
                        Value::String(s) => s.to_string(),
                        Value::Number(n) => n.to_string(),
                        _ => unreachable!(),
                    };
                    Some(s)
                }
                None => None,
            };
            (s, d, r)
        }
        _ => unreachable!(),
    }).into()
}

pub fn write_ads_number<T: Bounded + PartialOrd + ToPrimitive + FromPrimitive, W: WriteBytesExt>(
    data: T,
    w: &mut W,
    s: &Option<SubRange>,
) -> Result<(), std::io::Error> {
    let sr = match s {
        Some(sr) => (T::from_i64(sr.min).unwrap(), T::from_i64(sr.max).unwrap()),
        _ => (T::min_value(), T::max_value()),
    };
    let value = if data < sr.0 {
        sr.0
    } else if data > sr.1 {
        sr.1
    } else {
        data
    };
    if T::min_value().to_i64().unwrap_or(0) < 0 {
        w.write_int::<LittleEndian>(value.to_i64().unwrap(), std::mem::size_of::<T>())
    } else {
        w.write_uint::<LittleEndian>(value.to_u64().unwrap(), std::mem::size_of::<T>())
    }
}

fn read_number<T: Bounded + PartialOrd + ToPrimitive + FromPrimitive, Reader: ReadBytesExt>(
    r: &mut Reader,
) -> T {
    if T::min_value().to_i64().unwrap_or(0) < 0 {
        T::from_i64(
            r.read_int::<LittleEndian>(std::mem::size_of::<T>())
                .unwrap(),
        ).unwrap()
    } else {
        T::from_u64(
            r.read_uint::<LittleEndian>(std::mem::size_of::<T>())
                .unwrap(),
        ).unwrap()
    }
}

pub fn read_ads_number<T: Bounded + PartialOrd + ToPrimitive + FromPrimitive, R: ReadBytesExt>(
    r: &mut R,
    s: &Option<SubRange>,
) -> Value {
    let data = read_number::<T, R>(r);
    let sr = match s {
        Some(sr) => (T::from_i64(sr.min).unwrap(), T::from_i64(sr.max).unwrap()),
        _ => (T::min_value(), T::max_value()),
    };
    let value = if data < sr.0 {
        sr.0
    } else if data > sr.1 {
        sr.1
    } else {
        data
    };
    if T::min_value() < T::from_i64(0).unwrap() {
        Value::Number(value.to_i64().unwrap().into())
    } else {
        Value::Number(value.to_u64().unwrap().into())
    }
}
