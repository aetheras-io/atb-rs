use crate::types::{DateTime, Duration, Utc, Uuid};

use std::fmt::Display;

use jsonwebtoken::{self as jwt, crypto, errors::Error as JwtError, Algorithm};
use lazy_static::lazy_static;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[cfg(feature = "http")]
pub use self::actix_utils::*;
pub use jsonwebtoken::{DecodingKey, EncodingKey, Header};

lazy_static! {
    pub static ref HEADER_RS256: Header = Header::new(Algorithm::RS256);
    pub static ref HEADER_HS256: Header = Header::new(Algorithm::HS256);
}

pub type Signer<'a> = (&'a Header, &'a EncodingKey);
pub type Decoder<'a> = (&'a Header, &'a DecodingKey<'a>);

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
    pub fn new<S: AsRef<str>>(issuer: S, expiry_duration: Duration) -> Self {
        let iat = Utc::now();
        Builder(Claims {
            iss: issuer.as_ref().to_owned(),
            iat: iat.timestamp(),
            exp: (iat + expiry_duration).timestamp(),
            ..Default::default()
        })
    }

    pub fn subject<T: Display>(mut self, sub: T) -> Self {
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
        {
            log::warn!("custom value has keys that clash with default claim keys");
            self
        } else {
            self.0.custom = Some(value);
            self
        }
    }

    pub fn build(self) -> Claims {
        self.0
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

    pub fn custom(&self) -> Option<&serde_json::Value> {
        self.custom.as_ref()
    }

    /// Take and deserialize the custom values
    pub fn deserialize_custom<T: DeserializeOwned>(&mut self) -> Option<T> {
        self.custom
            .take()
            .and_then(|v| serde_json::from_value(v).ok())
    }

    /// Clone and deserialize the custom values
    pub fn deserialize_custom_cloned<T: DeserializeOwned>(&self) -> Option<T> {
        self.custom
            .clone()
            .and_then(|v| serde_json::from_value(v).ok())
    }

    pub fn encode(&self, signer: Signer) -> Result<String, JwtError> {
        jwt::encode(signer.0, self, signer.1)
    }

    // #TODO might be beneficial to get the error reason for debugging.
    // right now we will keep the same optional interface to get things going quickly
    pub fn decode<V: Fn(&Claims) -> bool>(
        token: &str,
        decoder: Decoder,
        validator: V,
    ) -> Option<Claims> {
        let (signature, message) = expect_two!(token.rsplitn(2, '.'));
        let (claims, header) = expect_two!(message.rsplitn(2, '.'));
        let header: Header = b64_decode_json(&header)?;
        let claims: Claims = b64_decode_json(&claims)?;

        if decoder.0.alg != header.alg
            || !validator(&claims)
            || !crypto::verify(signature, message, decoder.1, header.alg).ok()?
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

pub fn simple_validate(claims: &Claims) -> bool {
    const LEEWAY: i64 = 0;
    let now = Utc::now().timestamp();
    claims.exp > now - LEEWAY
}

#[cfg(feature = "http")]
pub mod actix_utils {
    use super::*;

    use std::{
        rc::Rc,
        result,
        task::{Context, Poll},
    };

    use actix_web::cookie::Cookie;
    use actix_web::dev::{Payload, Service, ServiceRequest, ServiceResponse, Transform};
    use actix_web::error::ErrorUnauthorized;
    use actix_web::http::header::AUTHORIZATION;
    use actix_web::{Error as ActixError, FromRequest, HttpMessage, HttpRequest};
    use futures::future::{self, Either, Ready};
    use time::OffsetDateTime;

    #[derive(thiserror::Error, Debug)]
    pub enum Error {
        #[error("missing authorization header")]
        MissingAuthHeader,

        #[error("invalid authorization header length")]
        InvalidHeaderLength,

        #[error("invalid authorization header encoding")]
        InvalidHeaderEncoding,

        #[error("invalid authorization scheme")]
        InvalidAuthScheme,

        #[error("missing authorization payload")]
        MissingAuthPayload,

        #[error("claims failed verification or decoding")]
        Failed,

        #[error("{0}")]
        Message(&'static str),
    }

    pub type Result<T, E = Error> = result::Result<T, E>;

    /// Transform/Builder for each actix engine
    pub struct JwtAuth<'a, EF, VF> {
        inner: Rc<JwtAuthInner<'a, EF, VF>>,
    }

    impl<'a, EF, VF> JwtAuth<'a, EF, VF> {
        pub fn new(extractor: EF, validator: VF, decoder: Decoder<'a>) -> Self {
            let inner = Rc::new(JwtAuthInner {
                extractor,
                validator,
                decoder,
            });
            JwtAuth { inner }
        }
    }

    /// Cloned settings for each thread's copy of the middleware
    pub struct JwtAuthInner<'a, EF, VF> {
        extractor: EF,
        validator: VF,
        decoder: Decoder<'a>,
    }

    impl<'a, EF, VF, S, B> Transform<S, ServiceRequest> for JwtAuth<'a, EF, VF>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&Claims) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
        S::Future: 'static,
        B: 'static,
    {
        type Response = ServiceResponse<B>;
        type Error = ActixError;
        type InitError = ();
        type Transform = JwtAuthMiddleware<'a, EF, VF, S>;
        type Future = Ready<Result<Self::Transform, Self::InitError>>;

        fn new_transform(&self, service: S) -> Self::Future {
            future::ok(JwtAuthMiddleware {
                service,
                inner: self.inner.clone(),
            })
        }
    }

    /// Middleware Implementation
    pub struct JwtAuthMiddleware<'a, EF, VF, S>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&Claims) -> bool,
    {
        service: S,
        inner: Rc<JwtAuthInner<'a, EF, VF>>,
    }

    impl<'a, EF, VF, S, B> Service<ServiceRequest> for JwtAuthMiddleware<'a, EF, VF, S>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&Claims) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = ActixError>,
        S::Future: 'static,
        B: 'static,
    {
        type Response = ServiceResponse<B>;
        type Error = ActixError;
        type Future = Either<Ready<Result<ServiceResponse<B>, ActixError>>, S::Future>;

        fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.service.poll_ready(cx)
        }

        fn call(&mut self, req: ServiceRequest) -> Self::Future {
            let suit = &self.inner;
            (suit.extractor)(&req)
                .and_then(|jwt| {
                    log::debug!("extracted jwt: {}", jwt);
                    Claims::decode(&jwt, suit.decoder, &suit.validator).ok_or(Error::Failed)
                })
                .map_or_else(
                    |e| {
                        log::debug!("jwt auth middleware error: {}", e);
                        Either::Left(future::err(ErrorUnauthorized("")))
                    },
                    |claims| {
                        req.extensions_mut().insert(claims);
                        Either::Right(self.service.call(req))
                    },
                )
        }
    }

    impl FromRequest for Claims {
        type Error = ActixError;
        type Future = Ready<Result<Self, ActixError>>;
        type Config = ();

        #[inline]
        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            match req.extensions_mut().remove::<Claims>() {
                Some(c) => future::ok(c),
                _ => future::err(ErrorUnauthorized("claims missing from extensions")),
            }
        }
    }

    pub fn from_bearer(req: &ServiceRequest) -> Result<String> {
        req.headers()
            .get(AUTHORIZATION)
            .ok_or(Error::MissingAuthHeader)
            .and_then(|header| {
                if header.len() < 8 {
                    return Err(Error::InvalidHeaderLength);
                }
                header
                    .to_str()
                    .map_err(|_| Error::InvalidHeaderEncoding)
                    .and_then(|parts| {
                        let mut parts = parts.splitn(2, ' ');
                        match parts.next() {
                            Some(scheme) if scheme == "Bearer" => (),
                            _ => return Err(Error::InvalidAuthScheme),
                        }
                        parts
                            .next()
                            .ok_or(Error::MissingAuthPayload)
                            .map(|jwt| jwt.to_owned())
                    })
            })
    }

    pub fn from_cookie_parts(
        parts: (&'static str, &'static str),
    ) -> impl Fn(&ServiceRequest) -> Result<String> {
        move |req: &ServiceRequest| match (req.cookie(parts.0), req.cookie(parts.1)) {
            (Some(token), Some(sig)) => Ok([token.value(), ".", sig.value()].concat()),
            _ => Err(Error::Message("unable to create jwt from split cookies")),
        }
    }

    pub fn to_cookie_parts<'a>(
        cookie_parts: (&'static str, &'static str),
        jwt: String,
        expiry: i64,
        fqdn: &'a str,
        debug: bool,
    ) -> Option<(Cookie<'a>, Cookie<'a>)> {
        let (token_part, sig_part) = jwt.rfind('.').map(|dot_position| {
            (
                jwt[..dot_position].to_owned(),
                jwt[dot_position + 1..].to_owned(),
            )
        })?;

        let (claim_part_key, sig_part_key) = cookie_parts;
        let claim_part = Cookie::build(claim_part_key, token_part)
            .domain(fqdn)
            .path("/")
            .secure(false)
            .http_only(false)
            .expires(OffsetDateTime::from_unix_timestamp(expiry))
            .finish();
        let sig_part = Cookie::build(sig_part_key, sig_part)
            .domain(fqdn)
            .path("/")
            .secure(!debug)
            .http_only(!debug)
            .expires(OffsetDateTime::from_unix_timestamp(expiry))
            .finish();

        Some((claim_part, sig_part))
    }
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

        let claims = Builder::new("denis", chrono::Duration::days(1))
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

        let mut claims = Builder::new("denis", chrono::Duration::days(1))
            .subject(Uuid::new_v4())
            .custom(custom.clone())
            .build();

        let custom_de: Custom = claims.deserialize_custom().unwrap();
        assert_eq!(custom, custom_de);
    }
}
