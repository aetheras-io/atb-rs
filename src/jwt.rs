use std::collections::HashMap;

use chrono::{Duration, Utc};
use jsonwebtoken::crypto;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
    #[serde(default)]
    pub iss: String,

    #[serde(default)]
    pub iat: i64,

    #[serde(default)]
    pub exp: i64,

    #[serde(default)]
    pub nbf: i64,

    #[serde(default)]
    pub sub: Uuid,

    #[serde(flatten)]
    pub custom: HashMap<String, serde_json::Value>,
}

// trait

/// Claims implementation
/// #NOTE As the JWT spec (https://tools.ietf.org/html/rfc7519#section-4.1.4) mentioned, the iat and exp fields
/// are of NumericDate type, which is a timestamp in second.
impl Claims {
    pub fn new(iss: &str, time_to_exp: Duration) -> Self {
        let iat = Utc::now();
        Self {
            iss: iss.to_owned(),
            iat: iat.timestamp(),
            exp: (iat + time_to_exp).timestamp(),
            ..Default::default()
        }
    }

    pub fn sub<S: Into<Uuid>>(mut self, sub: S) -> Self {
        self.sub = sub.into();
        self
    }

    // pub fn rle<S: Into<String>>(mut self, rle: S) -> Self {
    //     self.rle = rle.into();
    //     self
    // }

    // pub fn encode(self, private_key: &EncodingKey) -> String {
    //     jsonwebtoken::encode(&HEADER_RS256, &self, private_key)
    //         .expect("jwt keys should be properly loaded")
    // }

    // pub fn decode(token: &str, public_key: &DecodingKey, validation: &Validation) -> Option<Self> {
    //     match jwt::decode::<Self>(&token, public_key, &validation) {
    //         Ok(token_data) => Some(token_data.claims),
    //         Err(_) => None,
    //     }
    // }
}
