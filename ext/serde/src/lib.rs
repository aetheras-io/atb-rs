use serde::{Deserialize, Deserializer, Serializer};

pub mod i64_as_string {
    use super::*;

    pub fn serialize<S>(x: &i64, se: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        se.serialize_str(&x.to_string())
    }

    pub fn deserialize<'de, D>(de: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = String::deserialize(de)?;
        id.parse().map_err(serde::de::Error::custom)
    }
}

pub mod empty_string_none {
    use super::*;

    pub fn serialize<S>(x: &Option<String>, se: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match x {
            None => se.serialize_none(),
            Some(s) if s.is_empty() => se.serialize_none(),
            Some(s) => se.serialize_some(s),
        }
    }

    pub fn deserialize<'de, D, T>(de: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(de)?;
        match opt {
            None => Ok(None),
            Some(s) if s.is_empty() => Ok(None),
            _ => Ok(opt),
        }
    }
}

pub mod bytes_as_base64 {
    use super::*;
    // use base64::prelude::*;
    use base64::prelude::{Engine as _, BASE64_STANDARD};

    pub fn serialize<T, S>(bytes: &T, se: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: serde::Serializer,
    {
        se.serialize_str(&BASE64_STANDARD.encode(bytes.as_ref()))
    }

    pub fn deserialize<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(de)
            .and_then(|s| BASE64_STANDARD.decode(s).map_err(serde::de::Error::custom))
    }
}
