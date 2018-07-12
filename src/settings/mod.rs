mod map_deserialize;
use self::map_deserialize::N;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
pub struct Setting {
    pub connection_parameter: AmsConn,
    pub plc: Vec<PlcSetting>,
    pub versions: BTreeMap<N, VersionSetting>,
}

#[derive(Debug, Deserialize)]
pub struct AmsConn {
    pub ams_net_id: String,
    pub ams_port: u16,
}

#[derive(Debug, Deserialize)]
pub struct PlcSetting {
    pub version: u32,
    pub ip: String,
    pub ams_net_id: String,
    pub ams_port: u16,
}

#[derive(Debug, Deserialize)]
pub struct VersionSetting {
    pub path: String,
    pub symbol_names: Vec<String>,
}
