use std::collections::HashMap;
use types::{AdsPlcType, AdsType};

pub fn build_dependencies<'b>(key: &'b str, v: &mut Vec<String>, map: &HashMap<String, AdsType>) {
    match map.get(key) {
        Some(AdsType::Struct { properties, .. }) => {
            let _: Vec<_> = properties
                .iter()
                .map(|f| {
                    if let AdsPlcType::Other { ref reference, .. } = f.ty {
                        v.push(reference.clone());
                        build_dependencies(reference, v, map)
                    }
                })
                .collect();
        }
        Some(AdsType::Enum { .. }) => {
            v.push(key.to_string());
        }
        Some(AdsType::Array { ty, .. }) => if let AdsPlcType::Other { ref reference, .. } = ty {
            v.push(reference.clone());
            build_dependencies(reference, v, map);
        },
        _ => {}
    }
}
