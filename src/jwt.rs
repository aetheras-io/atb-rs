use crate::types::{DateTime, Duration, Utc, Uuid};

use jsonwebtoken::{self as jwt, crypto, errors::Error as JwtError, Algorithm};
use lazy_static::lazy_static;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "http")]
pub use actix_utils::*;
pub use jsonwebtoken::{DecodingKey, EncodingKey, Header};

lazy_static! {
    pub static ref HEADER_RS256: Header = Header::new(Algorithm::RS256);
    pub static ref HEADER_HS256: Header = Header::new(Algorithm::HS256);
}

/// Takes the result of a rsplit and ensure we only get 2 parts
/// this is taken from the jsonwebtoken crate for quick implementation.  
/// I think this is a totally unecessary "lispy" way of splitting
macro_rules! expect_two {
    ($iter:expr) => {{
        let mut i = $iter;
        match (i.next(), i.next(), i.next()) {
            (Some(first), Some(second), None) => (first, second),
            _ => return None,
        }
    }};
}

pub struct Builder(Claims);
impl Builder {
    pub fn new(expiry_duration: Duration) -> Self {
        let iat = Utc::now();
        Builder(Claims {
            iat: iat.timestamp(),
            exp: (iat + expiry_duration).timestamp(),
            ..Default::default()
        })
    }

    pub fn issuer<S: Into<String>>(mut self, issuer: S) -> Self {
        self.0.iss = issuer.into();
        self
    }

    pub fn subject<U: Into<String>>(mut self, sub: U) -> Self {
        self.0.sub = sub.into();
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
    pub fn custom(mut self, custom: serde_json::Value) -> Self {
        self.0.custom = Some(custom);
        self
    }
    pub fn build(self) -> Option<Claims> {
        if let Some(ref custom) = self.0.custom {
            if custom.get("iss").is_some()
                || custom.get("sub").is_some()
                || custom.get("iat").is_some()
                || custom.get("exp").is_some()
                || custom.get("nbf").is_some()
                || custom.get("aud").is_some()
            {
                return None;
            }
        }
        Some(self.0)
    }
}

/// #TODO make a custom error type for JWTs and Claims
#[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
pub struct Claims {
    #[serde(default, skip_serializing_if = "String::is_empty")]
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

    #[serde(flatten)]
    custom: Option<serde_json::Value>,
}

/// This is only used for serialize
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

    pub fn custom(&self) -> Option<&serde_json::Value> {
        self.custom.as_ref()
    }

    pub fn encode<T: JwtMeta>(&self) -> Result<String, JwtError> {
        jwt::encode(T::header(), self, T::encoding_key())
    }

    // #TODO might be beneficial to get the error reason for debugging.
    // right now we will keep the same optional interface to get things going quickly
    pub fn decode<F: Validator>(
        token: &str,
        public_key: &DecodingKey,
        validator: F,
    ) -> Option<Self> {
        let (signature, message) = expect_two!(token.rsplitn(2, '.'));
        let (claims, header) = expect_two!(message.rsplitn(2, '.'));
        let header: Header = b64_decode_json(&header)?;
        let claims: Claims = b64_decode_json(&claims)?;

        if !validator.validate(&header, &claims)
            || !crypto::verify(signature, message, public_key, header.alg).ok()?
        {
            None
        } else {
            Some(claims)
        }
    }
}

/// base64 decode into a string and then deserialize as json
fn b64_decode_json<T: DeserializeOwned>(input: &str) -> Option<T> {
    base64::decode_config(input, base64::URL_SAFE_NO_PAD)
        .ok()
        .and_then(|b| serde_json::from_slice::<T>(&b).ok())
}

pub fn validate_rsa256(header: &Header, claims: &Claims) -> bool {
    const LEEWAY: i64 = 0;
    let now = Utc::now().timestamp();
    HEADER_RS256.alg == header.alg && claims.exp < now - LEEWAY
}

pub fn validate_hmac256(header: &Header, claims: &Claims) -> bool {
    const LEEWAY: i64 = 0;
    let now = Utc::now().timestamp();
    HEADER_HS256.alg == header.alg && claims.exp < now - LEEWAY
}

pub trait JwtMeta {
    fn cookie_parts() -> (&'static str, &'static str) {
        ("access_token", "access_token_sig")
    }

    fn validate(header: &Header, claims: &Claims) -> bool {
        validate_rsa256(header, claims)
    }

    fn header() -> &'static Header {
        &HEADER_RS256
    }

    fn encoding_key() -> &'static EncodingKey;

    fn decoding_key() -> &'static DecodingKey<'static>;
}

pub trait Validator {
    fn validate(self, header: &Header, claims: &Claims) -> bool;
}

impl<F> Validator for F
where
    F: FnOnce(&Header, &Claims) -> bool,
{
    fn validate(self, header: &Header, claims: &Claims) -> bool {
        self(header, claims)
    }
}

#[cfg(feature = "http")]
mod actix_utils {
    use super::*;

    use std::marker::PhantomData;
    use std::ops::Deref;

    use actix_web::cookie::Cookie;
    use actix_web::dev::Payload;
    use actix_web::error::ErrorUnauthorized;
    use actix_web::{Error as ActixError, FromRequest, HttpMessage, HttpRequest};
    use futures::future;

    pub struct ClaimsFromCookies<T: JwtMeta> {
        inner: Claims,
        marker: PhantomData<T>,
    }

    impl<T: JwtMeta> ClaimsFromCookies<T> {
        fn into_inner(self) -> Claims {
            self.inner
        }
    }

    impl<T: JwtMeta> Deref for ClaimsFromCookies<T> {
        type Target = Claims;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl<T: JwtMeta> FromRequest for ClaimsFromCookies<T> {
        type Error = ActixError;
        type Future = future::Ready<Result<Self, ActixError>>;
        type Config = ();

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let (claim_part_key, sig_part_key) = T::cookie_parts();
            match (req.cookie(claim_part_key), req.cookie(sig_part_key)) {
                (Some(token), Some(sig)) => {
                    let jwt = format!("{}.{}", token.value(), sig.value());
                    Claims::decode(&jwt, T::decoding_key(), T::validate)
                        .ok_or(ErrorUnauthorized(""))
                }
                _ => Err(ErrorUnauthorized("")),
            }
            .map_or_else(future::err, |claims| {
                future::ok(ClaimsFromCookies {
                    inner: claims,
                    marker: PhantomData,
                })
            })
        }
    }

    pub fn to_cookie_parts<'a, T: JwtMeta>(
        jwt: String,
        fqdn: &'a str,
        debug: bool,
    ) -> Option<(Cookie, Cookie)> {
        let (token_part, sig_part) = jwt.rfind('.').map(|dot_position| {
            (
                jwt[..dot_position].to_owned(),
                jwt[dot_position + 1..].to_owned(),
            )
        })?;

        let (claim_part_key, sig_part_key) = T::cookie_parts();
        let claim_part = Cookie::build(claim_part_key, token_part)
            .domain(fqdn)
            .path("/")
            .secure(false)
            .http_only(false)
            .finish();
        let sig_part = Cookie::build(sig_part_key, sig_part)
            .domain(fqdn)
            .path("/")
            .secure(!debug)
            .http_only(!debug)
            .finish();

        Some((claim_part, sig_part))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_handles_duplicate_custom_fields() {
        unimplemented!();
    }

    #[test]
    fn it_handles_custom_fields() {
        let custom = serde_json::json!({
            "hello": "world"
        });
        let _claims = Builder::new(chrono::Duration::days(1))
            .issuer("hello")
            .subject(Uuid::new_v4().to_string())
            .custom(custom)
            .build();
        // #TODO check for errors
    }
}
