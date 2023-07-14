pub mod websockets;

use std::any;
use std::ops::Deref;

use actix_web::{dev::Payload, error::ErrorInternalServerError, FromRequest, HttpRequest};
use futures::future;

#[derive(Clone)]
pub struct SafeData<T: Clone + Send + Sync + 'static>(T);

impl<T: Clone + Send + Sync + 'static> SafeData<T> {
    /// Create new `SafeData` instance.
    pub fn new(state: T) -> Self {
        Self(state)
    }

    /// Convert to the internal `T`
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone + Send + Sync + 'static> Deref for SafeData<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone + Send + Sync + 'static> FromRequest for SafeData<T> {
    type Error = actix_web::error::Error;
    type Future = future::Ready<Result<Self, actix_web::error::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if let Some(st) = req.app_data::<SafeData<T>>() {
            future::ok(st.clone())
        } else {
            log::debug!(
                "Failed to construct App-level Data extractor. \
                 Request path: {:?} (type: {})",
                req.path(),
                any::type_name::<T>(),
            );
            future::err(ErrorInternalServerError(
                "App data is not configured, to configure use App::app_data()",
            ))
        }
    }
}

// #[cfg(feature = "jwt")]
pub mod jwt {
    use atb_types::jwt::{Claims as ClaimsInner, Decoder, Error as JwtError, FINGERPRINT_COOKIE};

    use std::rc::Rc;

    use actix_web::dev::{Payload, Service, ServiceRequest, ServiceResponse, Transform};
    use actix_web::error::ErrorUnauthorized;
    use actix_web::http::header::AUTHORIZATION;
    use actix_web::{body::BoxBody, cookie::Cookie};
    use actix_web::{Error as ActixError, FromRequest, HttpMessage, HttpRequest};
    use futures::future::{self, Either, Ready};
    use time::OffsetDateTime;

    pub struct Claims(ClaimsInner);

    impl Claims {
        /// Convert to the internal `T`
        pub fn into_inner(self) -> ClaimsInner {
            self.0
        }
    }

    impl std::ops::Deref for Claims {
        type Target = ClaimsInner;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

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

        #[error("decode: {0}")]
        Decode(#[from] JwtError),

        #[error("{0}")]
        Message(&'static str),
    }

    pub type Result<T, E = Error> = std::result::Result<T, E>;

    /// Transform/Builder for each actix engine
    pub struct JwtAuth<'a, EF, VF> {
        inner: Rc<JwtAuthInner<'a, EF, VF>>,
    }

    impl<'a, EF, VF> JwtAuth<'a, EF, VF> {
        /// fn issue_jwt<'a>(fqdn: &'a str) -> (ClaimsInner, Cookie<'a>) {
        ///     use atb_types::jwt::Builder;
        ///     use atb_types::Duration;
        ///
        ///     let duration = Duration::hours(8);
        ///     let (claims, fingerprint) = Builder::new("eg", duration)
        ///         .subject("someone")
        ///         .audience(vec!["aud".to_owned()])
        ///         .build_fingerprinted();
        ///
        ///     let cookie = fingerprint_cookie(fqdn, fingerprint, claims.expiry()).unwrap();
        ///
        ///     (claims, cookie)
        /// }
        ///
        /// fn validate(req: &ServiceRequest, claims: &Claims) {
        ///     validate_expiry(claims);
        ///     let fp = extract_fingerprint(req);
        ///     claims.verify_fingerprint(fp);
        /// }
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

    impl<'a, EF, VF, S> Transform<S, ServiceRequest> for JwtAuth<'a, EF, VF>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&ServiceRequest, &ClaimsInner) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = ActixError>,
        S::Future: 'static,
    {
        type Response = ServiceResponse<BoxBody>;
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
        VF: Fn(&ServiceRequest, &ClaimsInner) -> bool,
    {
        service: S,
        inner: Rc<JwtAuthInner<'a, EF, VF>>,
    }

    impl<'a, EF, VF, S> Service<ServiceRequest> for JwtAuthMiddleware<'a, EF, VF, S>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&ServiceRequest, &ClaimsInner) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = ActixError>,
        S::Future: 'static,
    {
        type Response = ServiceResponse<BoxBody>;
        type Error = ActixError;
        type Future = Either<Ready<Result<ServiceResponse<BoxBody>, ActixError>>, S::Future>;

        actix_web::dev::forward_ready!(service);

        fn call(&self, req: ServiceRequest) -> Self::Future {
            let suit = &self.inner;

            match (suit.extractor)(&req).and_then(|jwt| {
                log::debug!("extracted jwt: {}", jwt);
                ClaimsInner::decode(&jwt, suit.decoder).map_err(Into::into)
            }) {
                Ok(inner) => {
                    if (suit.validator)(&req, &inner) {
                        req.extensions_mut().insert(Claims(inner));
                        Either::Right(self.service.call(req))
                    } else {
                        Either::Left(future::ok(req.error_response(ErrorUnauthorized(""))))
                    }
                }
                Err(e) => {
                    log::debug!("jwt auth middleware error: {}", e);
                    Either::Left(future::ok(req.error_response(ErrorUnauthorized(""))))
                }
            }
        }
    }

    impl FromRequest for Claims {
        type Error = ActixError;
        type Future = Ready<Result<Self, ActixError>>;

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

        let expiry = OffsetDateTime::from_unix_timestamp(expiry).ok()?;
        let (claim_part_key, sig_part_key) = cookie_parts;
        let claim_part = Cookie::build(claim_part_key, token_part)
            .domain(fqdn)
            .path("/")
            .secure(false)
            .http_only(false)
            .expires(Some(expiry))
            .finish();
        let sig_part = Cookie::build(sig_part_key, sig_part)
            .domain(fqdn)
            .path("/")
            .secure(!debug)
            .http_only(!debug)
            .expires(Some(expiry))
            .finish();

        Some((claim_part, sig_part))
    }

    pub fn extract_fingerprint(req: &ServiceRequest) -> Option<Cookie<'static>> {
        req.cookie(FINGERPRINT_COOKIE)
    }

    pub fn fingerprint_cookie<'a>(
        fqdn: &'a str,
        encoded_fingerprint: String,
        expiry: i64,
    ) -> Result<Cookie<'a>> {
        let expiry = OffsetDateTime::from_unix_timestamp(expiry)
            .map_err(|_| Error::Message("invalid expiry i64 value"))?;
        Ok(Cookie::build(FINGERPRINT_COOKIE, encoded_fingerprint)
            .domain(fqdn)
            .path("/")
            .secure(true)
            .http_only(true)
            .expires(Some(expiry))
            .finish())
    }
}
