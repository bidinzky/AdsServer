extern crate ads_types as types;
extern crate chashmap;
extern crate quickxml_to_serde;
extern crate serde_json;
extern crate settings;

mod helper;

use chashmap::CHashMap;
use serde_json::Value;
use settings::VersionSetting;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
};

use self::helper::build_dependencies;
use types::helper::{number_from_value, type_from_value};
use types::{AdsPlcType, AdsType, AdsVersion, Name, Symbol};

fn xml_to_json<R: BufRead>(r: R) -> Value {
    let e = quickxml_to_serde::get_root(r).unwrap();
    quickxml_to_serde::xml_to_map(&e)
}

pub fn read_tpy(conf: &VersionSetting) -> AdsVersion {
    let f = BufReader::new(File::open(&conf.path).unwrap());
    let e = xml_to_json(f);
    let symbols: CHashMap<String, Symbol> = (&e["PlcProjectInfo"]["Symbols"]["Symbol"])
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let index_group = number_from_value(&s["IGroup"]);
            let index_offset = number_from_value(&s["IOffset"]);
            let name: Name = (&s["Name"]).into();
            let ty = type_from_value(&s["Type"], None);
            Symbol {
                index_group,
                index_offset,
                name,
                ty,
            }
        })
        .filter(|f| conf.symbol_names.contains(&f.name.text))
        .filter_map(|f| match f.ty.clone() {
            AdsPlcType::Other { ref reference, .. } => Some((reference.to_string(), f)),
            _ => None,
        })
        .collect();
    let data_types = e["PlcProjectInfo"]["DataTypes"]["DataType"]
        .as_array()
        .unwrap();
    let search_vec = vec!["ST_ADS_FROM_BC", "ST_ADS_TO_BC", "ST_RETAIN_DATA"];
    let search_index = CHashMap::with_capacity(search_vec.len());
    let map: HashMap<String, AdsType> = data_types
        .iter()
        .filter_map(|f| {
            let name: Name = (&f["Name"]).into();
            match name.decoration {
                Some(d) => match AdsType::from_value(f) {
                    Some(v) => Some((d, v)),
                    None => None,
                },
                None => None,
            }
        })
        .collect();
    let mut dep = Vec::new();
    let _: Vec<_> = map
        .iter()
        .filter_map(|(k, v)| match v {
            AdsType::Struct { name, .. } => if search_vec.contains(&name.as_ref()) {
                search_index.insert(name.to_string(), k.to_string());
                Some(k)
            } else {
                None
            },
            _ => None,
        })
        .map(|f| f.as_ref())
        .map(|f: &str| {
            dep.push(f.to_string());
            build_dependencies(f, &mut dep, &map);
        })
        .collect();
    let fmap: CHashMap<String, AdsType> = map
        .into_iter()
        .filter_map(|(k, v)| if dep.contains(&k) { Some((k, v)) } else { None })
        .collect();
    AdsVersion {
        map: fmap,
        search_index,
        symbols,
    }
}
