use crate::{DateTime, Duration, Utc, Uuid};

use std::fmt::Display;

use base64::prelude::*;
use jsonwebtoken::{self as jwt, crypto, errors::Error as JwtError, Algorithm};
use lazy_static::lazy_static;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use jsonwebtoken::{DecodingKey, EncodingKey, Header};

pub const FINGERPRINT_COOKIE: &str = "__Atb-Secure-Fpt";

lazy_static! {
    pub static ref HEADER_RS256: Header = Header::new(Algorithm::RS256);
    pub static ref HEADER_HS256: Header = Header::new(Algorithm::HS256);
}

pub type Signer<'a> = (&'a Header, &'a EncodingKey);
pub type Decoder<'a> = (&'a Header, &'a DecodingKey);

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

/// Takes the result of a rsplit and ensure we only get 2 parts
/// this is taken from the jsonwebtoken crate for quick implementation.  
/// I think this is a totally unecessary "lispy" way of splitting
macro_rules! split_two {
    ($iter:expr) => {{
        let mut i = $iter;
        match (i.next(), i.next(), i.next()) {
            (Some(first), Some(second), None) => (first, second),
            _ => return Err(Error::InvalidToken),
        }
    }};
}

pub struct Builder(Claims);

impl Builder {
    pub fn new<S: AsRef<str>>(issuer: S, expiry_duration: Duration) -> Self {
        let iat = Utc::now();
        Builder(Claims {
            iss: issuer.as_ref().to_owned(),
            iat: iat.timestamp(),
            exp: (iat + expiry_duration).timestamp(),
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

    /// add custom serialization values on top of the default
    pub fn custom<T: Serialize>(mut self, data: T) -> Self {
        let value = serde_json::to_value(data).expect("custom claims data should serialize. qed");
        if value.get("iss").is_some()
            || value.get("sub").is_some()
            || value.get("iat").is_some()
            || value.get("exp").is_some()
            || value.get("nbf").is_some()
            || value.get("aud").is_some()
            || value.get("fpt").is_some()
        {
            self
        } else {
            self.0.custom = Some(value);
            self
        }
    }

    pub fn build(self) -> Claims {
        self.0
    }

    pub fn build_fingerprinted(mut self) -> (Claims, String) {
        let entropy = vec![0, 1];
        let mut hasher = Sha256::new();
        hasher.update(&entropy);
        self.0.fpt = Some(BASE64_STANDARD.encode(hasher.finalize()));

        (self.0, BASE64_STANDARD.encode(entropy))
    }
}

/// #TODO make a custom error type for JWTs and Claims
#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
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
    custom: Option<serde_json::Value>,
}

/// This is only used for serde ```skip_serialize_if```
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(num: &i64) -> bool {
    *num == 0
}

/// Claims implementation
/// #NOTE As the JWT spec (https://tools.ietf.org/html/rfc7519#section-4.1.4) mentioned, the iat and exp fields
/// are of NumericDate type, which is a timestamp in second.
impl Claims {
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

    pub fn custom(&self) -> Option<&serde_json::Value> {
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

    pub fn deserialize_custom<T: DeserializeOwned>(&self) -> Option<T> {
        self.custom
            .as_ref()
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn encode(&self, signer: Signer) -> Result<String, JwtError> {
        jwt::encode(signer.0, self, signer.1)
    }

    // #TODO might be beneficial to get the error reason for debugging.
    // right now we will keep the same optional interface to get things going quickly
    pub fn decode(token: &str, decoder: Decoder) -> Result<Claims> {
        let (signature, message) = split_two!(token.rsplitn(2, '.'));
        let (claims, header) = split_two!(message.rsplitn(2, '.'));
        let header: Header = b64_decode_json(&header)?;

        if decoder.0.alg != header.alg
            || !crypto::verify(signature, message.as_bytes(), decoder.1, header.alg)?
        {
            Err(Error::Algorithm)
        } else {
            b64_decode_json(&claims)
        }
    }
}

/// base64 decode into a string and then deserialize as json
fn b64_decode_json<T: DeserializeOwned>(input: &str) -> Result<T> {
    let bytes = BASE64_URL_SAFE.decode(input)?;
    serde_json::from_slice::<T>(&bytes).map_err(Into::into)
}

pub fn validate_expiry(claims: &Claims) -> bool {
    const LEEWAY: i64 = 0;
    let now = Utc::now().timestamp();
    claims.exp > now - LEEWAY
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_handles_clashing_custom_fields() {
        #[derive(PartialEq, Debug, Serialize, Deserialize)]
        struct Custom {
            iss: String,
        }
        let custom = Custom {
            iss: "notdenis".to_owned(),
        };

        let claims = Builder::new("denis", Duration::days(1))
            .subject(Uuid::new_v4())
            .custom(custom)
            .build();
        assert!(claims.custom().is_none());
    }

    #[test]
    fn it_handles_custom_fields() {
        #[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
        struct Custom {
            hello: String,
        }
        let custom = Custom {
            hello: "world".to_owned(),
        };

        let claims = Builder::new("denis", Duration::days(1))
            .subject(Uuid::new_v4().to_string())
            .custom(custom.clone())
            .build();

        let custom_de: Custom = claims.deserialize_custom().unwrap();
        assert_eq!(custom, custom_de);
    }
}
