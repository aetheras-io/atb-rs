use crate::{DateTime, Duration, Utc, Uuid};

use std::fmt::Display;

use base64::prelude::*;
use jsonwebtoken::{
    self as jwt, crypto, errors::Error as JwtError, Algorithm, DecodingKey, EncodingKey, Header,
};
use once_cell::sync::Lazy;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use jsonwebtoken;

pub const FINGERPRINT_COOKIE: &str = "__Atb-Secure-Fpt";

pub static HEADER_RS256: Lazy<Header> = Lazy::new(|| Header::new(Algorithm::RS256));
pub static HEADER_HS256: Lazy<Header> = Lazy::new(|| Header::new(Algorithm::HS256));

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid jwt token structure")]
    InvalidToken,

    #[error("mismatched jwt algorithm header")]
    Algorithm,

    #[error("base64: {0}")]
    Base64Decode(#[from] base64::DecodeError),

    #[error("serde json : {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("jwt crypto: {0}")]
    Crypto(#[from] JwtError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct Builder<T>(Claims<T>);

impl Builder<NoCustom> {
    pub fn new<S: AsRef<str>>(issuer: S, expiry_duration: Duration) -> Self {
        let iat = Utc::now();
        Builder(Claims {
            iss: issuer.as_ref().to_owned(),
            iat: iat.timestamp(),
            exp: (iat + expiry_duration).timestamp(),
            ..Default::default()
        })
    }
}

impl<T> Builder<T> {
    /// #WARNING make sure custom fields do not clash with the default claims fields
    /// checks for this will only happen in debug mode.
    pub fn with_custom<S: AsRef<str>>(issuer: S, expiry_duration: Duration, custom: T) -> Builder<T>
    where
        T: Serialize + Default,
    {
        #[cfg(debug_assertions)]
        {
            let value =
                serde_json::to_value(&custom).expect("custom claims data should serialize. qed");
            if value.get("iss").is_some()
                || value.get("sub").is_some()
                || value.get("iat").is_some()
                || value.get("exp").is_some()
                || value.get("nbf").is_some()
                || value.get("aud").is_some()
                || value.get("fpt").is_some()
            {
                panic!("duplicate field found in custom claims");
            }
        }

        let iat = Utc::now();
        Builder(Claims {
            iss: issuer.as_ref().to_owned(),
            iat: iat.timestamp(),
            exp: (iat + expiry_duration).timestamp(),
            custom: Some(custom),
            ..Default::default()
        })
    }

    pub fn subject(mut self, sub: impl Display) -> Self {
        self.0.sub = sub.to_string();
        self
    }

    pub fn not_before(mut self, time: DateTime) -> Self {
        self.0.nbf = time.timestamp();
        self
    }

    pub fn audience(mut self, audience: Vec<String>) -> Self {
        self.0.aud = audience;
        self
    }

    pub fn build(self) -> Claims<T> {
        self.0
    }

    pub fn build_fingerprinted(mut self) -> (Claims<T>, String) {
        use rand::Rng;
        let mut rng = rand::rng();
        let mut entropy = [0u8; 32];
        rng.fill(&mut entropy[..]);

        let mut hasher = Sha256::new();
        hasher.update(entropy);
        self.0.fpt = Some(BASE64_STANDARD.encode(hasher.finalize()));

        (self.0, BASE64_STANDARD.encode(entropy))
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NoCustom;

/// #TODO make a custom error type for JWTs and Claims
#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Claims<T = NoCustom> {
    #[serde(default)]
    iss: String,

    #[serde(default)]
    sub: String,

    #[serde(default)]
    iat: i64,

    #[serde(default, skip_serializing_if = "is_zero")]
    nbf: i64,

    #[serde(default)]
    exp: i64,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    aud: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    fpt: Option<String>,

    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    custom: Option<T>,
}

/// This is only used for serde ```skip_serialize_if```
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(num: &i64) -> bool {
    *num == 0
}

impl Claims<NoCustom> {
    pub fn decode(token: &str, header: &Header, decoding_key: &DecodingKey) -> Result<Self> {
        decode(token, header, decoding_key)
    }
}

/// Claims implementation
/// #NOTE As the JWT spec (https://tools.ietf.org/html/rfc7519#section-4.1.4) mentioned, the iat and exp fields
/// are of NumericDate type, which is a timestamp in second.
impl<T> Claims<T>
where
    T: Serialize + DeserializeOwned,
{
    pub fn issuer(&self) -> &str {
        &self.iss
    }

    pub fn subject(&self) -> &str {
        &self.sub
    }

    pub fn subject_as_uuid(&self) -> Result<Uuid, uuid::Error> {
        Uuid::parse_str(&self.sub)
    }

    pub fn issued_at(&self) -> i64 {
        self.iat
    }

    pub fn expiry(&self) -> i64 {
        self.exp
    }

    pub fn not_before(&self) -> i64 {
        self.nbf
    }

    pub fn audience(&self) -> &[String] {
        &self.aud
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.fpt.as_deref()
    }

    pub fn custom(&self) -> Option<&T> {
        self.custom.as_ref()
    }

    pub fn verify_fingerprint(&self, encoded: &str) -> bool {
        let Some(ref hash) = self.fpt else {
            return false;
        };

        let Ok(entropy) = BASE64_STANDARD.decode(encoded) else {
            return false;
        };

        let mut hasher = Sha256::new();
        hasher.update(entropy);
        &BASE64_STANDARD.encode(hasher.finalize()) == hash
    }

    pub fn encode(&self, header: &Header, encoding_key: &EncodingKey) -> Result<String, Error> {
        jwt::encode(header, self, encoding_key).map_err(Into::into)
    }

    pub fn decode_custom(
        token: &str,
        header: &Header,
        decoding_key: &DecodingKey,
    ) -> Result<Claims<T>> {
        decode(token, header, decoding_key)
    }
}

// #TODO might be beneficial to get the error reason for debugging.
// right now we will keep the same optional interface to get things going quickly
pub fn decode<T>(token: &str, header: &Header, decoding_key: &DecodingKey) -> Result<Claims<T>>
where
    T: DeserializeOwned,
{
    let (signature, message) = split_two(token.rsplitn(2, '.'))?;
    let (claims, header_part) = split_two(message.rsplitn(2, '.'))?;
    let claims_header: Header = b64_decode_json(header_part)?;

    if claims_header.alg != header.alg
        || !crypto::verify(signature, message.as_bytes(), decoding_key, header.alg)?
    {
        Err(Error::Algorithm)
    } else {
        b64_decode_json(claims)
    }
}

/// base64 decode into a string and then deserialize as json
fn b64_decode_json<T: DeserializeOwned>(input: &str) -> Result<T> {
    let bytes = BASE64_URL_SAFE_NO_PAD.decode(input)?;
    serde_json::from_slice::<T>(&bytes).map_err(Into::into)
}

pub fn validate_expiry(claims: &Claims) -> bool {
    const LEEWAY: i64 = 0;
    let now = Utc::now().timestamp();
    claims.exp > now - LEEWAY
}

fn split_two<T, I>(mut iter: I) -> Result<(T, T), Error>
where
    I: Iterator<Item = T>,
{
    match (iter.next(), iter.next(), iter.next()) {
        (Some(first), Some(second), None) => Ok((first, second)),
        _ => Err(Error::InvalidToken),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn it_handles_clashing_custom_fields() {
        #[derive(PartialEq, Debug, Serialize, Deserialize, Default)]
        struct Custom {
            iss: String,
        }
        let custom = Custom {
            iss: "notdenis".to_owned(),
        };

        let _claims = Builder::with_custom("denis", Duration::days(1), custom)
            .subject(Uuid::new_v4())
            .build();
    }

    #[test]
    fn it_can_default_ergonomically() {
        let claims = Builder::new("denis", Duration::days(1))
            .subject(Uuid::new_v4().to_string())
            .build();
        let claims_ser = serde_json::to_string(&claims).unwrap();
        let claims_de: Claims = serde_json::from_str(&claims_ser).unwrap();

        assert_eq!(claims, claims_de);
    }

    #[test]
    fn it_handles_custom_fields() {
        #[derive(PartialEq, Debug, Serialize, Deserialize, Clone, Default)]
        struct Custom {
            hello: String,
        }
        let custom = Custom {
            hello: "world".to_owned(),
        };

        let claims = Builder::with_custom("denis", Duration::days(1), custom)
            .subject(Uuid::new_v4().to_string())
            .build();

        let claims_ser = serde_json::to_string(&claims).unwrap();
        let claims_de: Claims<Custom> = serde_json::from_str(&claims_ser).unwrap();
        assert_eq!(claims, claims_de);

        let claims_no_custom_from_custom_claim: Claims = serde_json::from_str(&claims_ser).unwrap();
        assert!(claims_no_custom_from_custom_claim.custom().is_none());
    }
}
