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

    pub fn de_empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        use serde::de::IntoDeserializer;
        let opt = Option::<String>::deserialize(de)?;
        let opt = opt.as_deref();
        match opt {
            None | Some("") => Ok(None),
            Some(s) => T::deserialize(s.into_deserializer()).map(Some),
        }
    }
}
