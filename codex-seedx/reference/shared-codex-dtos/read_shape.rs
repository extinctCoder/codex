use std::fmt;

use serde::Deserialize;

pub const DEFAULT_LIST: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ReadShape {
    List(String),
    Detail(String),
}

impl fmt::Display for ReadShape {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::List(surface_name) => write!(formatter, "list:{surface_name}"),
            Self::Detail(surface_name) => write!(formatter, "detail:{surface_name}"),
        }
    }
}

impl<'de> Deserialize<'de> for ReadShape {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "lowercase")]
        enum Helper {
            List(String),
            Detail(String),
        }
        Ok(match Helper::deserialize(deserializer)? {
            Helper::List(surface_name) => Self::List(surface_name),
            Helper::Detail(surface_name) => Self::Detail(surface_name),
        })
    }
}
