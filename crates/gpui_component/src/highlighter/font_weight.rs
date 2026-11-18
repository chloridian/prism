use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

use super::FontWeightContent;

impl Serialize for FontWeightContent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u16(*self as u16)
    }
}

impl<'de> Deserialize<'de> for FontWeightContent {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u16::deserialize(deserializer)? {
            100 => Ok(Self::Thin),
            200 => Ok(Self::ExtraLight),
            300 => Ok(Self::Light),
            400 => Ok(Self::Normal),
            500 => Ok(Self::Medium),
            600 => Ok(Self::Semibold),
            700 => Ok(Self::Bold),
            800 => Ok(Self::ExtraBold),
            900 => Ok(Self::Black),
            other => Err(D::Error::custom(format!("invalid font weight {other}"))),
        }
    }
}
