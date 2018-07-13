mod map_deserialize;
use self::map_deserialize::N;
use std::collections::BTreeMap;
use super::networking::ToPlcConn;

#[derive(Debug, Deserialize, Serialize)]
pub struct Setting {
    pub connection_parameter: AmsConn,
    pub plc: Vec<PlcSetting>,
    pub versions: BTreeMap<N, VersionSetting>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AmsConn {
    pub ams_net_id: String,
    pub ams_port: u16,
}

impl<'a> ToPlcConn for &'a AmsConn {
    fn try_into_plc_conn(&self) -> Option<[u8; 8]> {
        let net_id: Vec<_> = self.
            ams_net_id
            .split(".")
            .map(|x| u8::from_str_radix(x, 10).unwrap())
            .collect();
        let mut d = [0u8; 8];
        for i in 0..6 {
            d[i] = net_id[i];
        }
        d[7] = ((self.ams_port >> 8) & 0xff) as u8;
        d[6] = (self.ams_port & 0xff) as u8;
        Some(d)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PlcSetting {
    pub version: u32,
    pub ip: String,
    pub ams_net_id: String,
    pub ams_port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct VersionSetting {
    pub path: String,
    pub symbol_names: Vec<String>,
}
