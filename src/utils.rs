#[cfg(feature = "serde_utils")]
pub mod serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn ser_i64_as_string<S>(x: &i64, se: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        se.serialize_str(&x.to_string())
    }

    pub fn de_string_as_i64<'de, D>(de: D) -> Result<i64, D::Error>
    where
        D: Deserializer<'de>,
    {
        let id = String::deserialize(de)?;
        id.parse().map_err(serde::de::Error::custom)
    }

    pub fn ser_empty_string_as_none<S>(x: &Option<String>, se: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match x {
            None => se.serialize_none(),
            Some(s) if s.len() == 0 => se.serialize_none(),
            Some(s) => se.serialize_some(s),
        }
    }

    pub fn de_empty_as_none<'de, D, T>(de: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(de)?;
        match opt {
            None => Ok(None),
            Some(s) if &s == "" => Ok(None),
            _ => Ok(opt),
        }
    }

    pub fn ser_bytes_as_base64<T, S>(bytes: &T, se: S) -> Result<S::Ok, S::Error>
    where
        T: AsRef<[u8]>,
        S: serde::Serializer,
    {
        se.serialize_str(&base64::encode(bytes.as_ref()))
    }

    pub fn de_base64_as_bytes<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let b64_str = String::deserialize(de)?;
        base64::decode(b64_str).map_err(serde::de::Error::custom)
    }
}
