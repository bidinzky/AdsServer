use serde::de::{self, Deserialize, Deserializer};
use serde::ser::{Serialize, Serializer};
use std::cmp::Ordering;
use std::ops::Deref;

// Number that can be used as a map key. Infinity and NaN are not allowed.
#[derive(Debug, PartialEq, PartialOrd, Copy, Clone)]
pub struct N(u32);

impl Eq for N {}
impl Ord for N {
    fn cmp(&self, other: &N) -> Ordering {
        match self.partial_cmp(&other) {
            Some(ord) => ord,
            None => unreachable!(),
        }
    }
}

impl<'de> Deserialize<'de> for N {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let f = s.parse::<u32>().map_err(de::Error::custom)?;
        Ok(f.into())
    }
}

impl Serialize for N {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u32(self.0)
    }
}

impl From<u32> for N {
    fn from(u: u32) -> Self {
        N(u)
    }
}

impl<'a> From<&'a N> for u32 {
    fn from(n: &'a N) -> Self {
        n.0
    }
}

impl Deref for N {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
