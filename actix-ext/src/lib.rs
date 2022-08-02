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

#[cfg(feature = "jwt")]
pub mod jwt {
    use atb_types::jwt::{Claims as ClaimsInner, Decoder};

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

        pub fn inner(&self) -> &ClaimsInner {
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

        #[error("claims failed verification or decoding")]
        Failed,

        #[error("{0}")]
        Message(&'static str),
    }

    pub type Result<T, E = Error> = std::result::Result<T, E>;

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

    impl<'a, EF, VF, S> Transform<S, ServiceRequest> for JwtAuth<'a, EF, VF>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&ClaimsInner) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = ActixError>,
        S::Future: 'static,
        // B: 'static,
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
        VF: Fn(&ClaimsInner) -> bool,
    {
        service: S,
        inner: Rc<JwtAuthInner<'a, EF, VF>>,
    }

    impl<'a, EF, VF, S> Service<ServiceRequest> for JwtAuthMiddleware<'a, EF, VF, S>
    where
        EF: Fn(&ServiceRequest) -> Result<String>,
        VF: Fn(&ClaimsInner) -> bool,
        S: Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = ActixError>,
        S::Future: 'static,
        // B: 'static,
    {
        type Response = ServiceResponse<BoxBody>;
        type Error = ActixError;
        // type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;
        // type Future = Ready<Result<Self::Response, Self::Error>>;
        type Future = Either<Ready<Result<ServiceResponse<BoxBody>, ActixError>>, S::Future>;

        actix_service::forward_ready!(service);

        fn call(&self, req: ServiceRequest) -> Self::Future {
            let suit = &self.inner;

            match (suit.extractor)(&req).and_then(|jwt| {
                log::debug!("extracted jwt: {}", jwt);
                ClaimsInner::decode(&jwt, suit.decoder, &suit.validator)
                    .map(Claims)
                    .ok_or(Error::Failed)
            }) {
                Ok(claims) => {
                    req.extensions_mut().insert(claims);
                    Either::Right(self.service.call(req))
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
}

///#HACK juniper actix isn't updated for beta channel actix for Tokio 1.0
/// https://github.com/aetheras-io/atb-rs/issues/1
#[cfg(feature = "graphql-juniper")]
pub mod juniper_actix {
    use actix_web::{
        error::{ErrorBadRequest, ErrorMethodNotAllowed, ErrorUnsupportedMediaType},
        http::{header::CONTENT_TYPE, Method},
        web, Error, FromRequest, HttpRequest, HttpResponse,
    };
    use juniper::{
        http::{
            graphiql::graphiql_source, playground::playground_source, GraphQLBatchRequest,
            GraphQLRequest,
        },
        ScalarValue,
    };
    use serde::Deserialize;

    #[derive(Deserialize, Clone, PartialEq, Debug)]
    #[serde(deny_unknown_fields)]
    struct GetGraphQLRequest {
        query: String,
        #[serde(rename = "operationName")]
        operation_name: Option<String>,
        variables: Option<String>,
    }

    impl<S> From<GetGraphQLRequest> for GraphQLRequest<S>
    where
        S: ScalarValue,
    {
        fn from(get_req: GetGraphQLRequest) -> Self {
            let GetGraphQLRequest {
                query,
                operation_name,
                variables,
            } = get_req;
            let variables = match variables {
                Some(variables) => Some(serde_json::from_str(&variables).unwrap()),
                None => None,
            };
            Self::new(query, operation_name, variables)
        }
    }

    /// Actix Web GraphQL Handler for GET and POST requests
    pub async fn graphql_handler<Query, Mutation, Subscription, CtxT, S>(
        schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
        context: &CtxT,
        req: HttpRequest,
        payload: actix_web::web::Payload,
    ) -> Result<HttpResponse, Error>
    where
        Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Query::TypeInfo: Sync,
        Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Mutation::TypeInfo: Sync,
        Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
        Subscription::TypeInfo: Sync,
        CtxT: Sync,
        S: ScalarValue + Send + Sync,
    {
        match *req.method() {
            Method::POST => post_graphql_handler(schema, context, req, payload).await,
            Method::GET => get_graphql_handler(schema, context, req).await,
            _ => Err(ErrorMethodNotAllowed(
                "GraphQL requests can only be sent with GET or POST",
            )),
        }
    }
    /// Actix GraphQL Handler for GET requests
    pub async fn get_graphql_handler<Query, Mutation, Subscription, CtxT, S>(
        schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
        context: &CtxT,
        req: HttpRequest,
    ) -> Result<HttpResponse, Error>
    where
        Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Query::TypeInfo: Sync,
        Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Mutation::TypeInfo: Sync,
        Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
        Subscription::TypeInfo: Sync,
        CtxT: Sync,
        S: ScalarValue + Send + Sync,
    {
        let get_req = web::Query::<GetGraphQLRequest>::from_query(req.query_string())?;
        let req = GraphQLRequest::from(get_req.into_inner());
        let gql_response = req.execute(schema, context).await;
        let body_response = serde_json::to_string(&gql_response)?;
        let mut response = match gql_response.is_ok() {
            true => HttpResponse::Ok(),
            false => HttpResponse::BadRequest(),
        };
        Ok(response
            .content_type("application/json")
            .body(body_response))
    }

    /// Actix GraphQL Handler for POST requests
    pub async fn post_graphql_handler<Query, Mutation, Subscription, CtxT, S>(
        schema: &juniper::RootNode<'static, Query, Mutation, Subscription, S>,
        context: &CtxT,
        req: HttpRequest,
        payload: actix_web::web::Payload,
    ) -> Result<HttpResponse, Error>
    where
        Query: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Query::TypeInfo: Sync,
        Mutation: juniper::GraphQLTypeAsync<S, Context = CtxT>,
        Mutation::TypeInfo: Sync,
        Subscription: juniper::GraphQLSubscriptionType<S, Context = CtxT>,
        Subscription::TypeInfo: Sync,
        CtxT: Sync,
        S: ScalarValue + Send + Sync,
    {
        let content_type_header = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|hv| hv.to_str().ok());
        let req = match content_type_header {
            Some("application/json") => {
                let body = String::from_request(&req, &mut payload.into_inner()).await?;
                serde_json::from_str::<GraphQLBatchRequest<S>>(&body).map_err(ErrorBadRequest)
            }
            Some("application/graphql") => {
                let body = String::from_request(&req, &mut payload.into_inner()).await?;
                Ok(GraphQLBatchRequest::Single(GraphQLRequest::new(
                    body, None, None,
                )))
            }
            _ => Err(ErrorUnsupportedMediaType(
                "GraphQL requests should have content type `application/json` or `application/graphql`",
            )),
        }?;
        let gql_batch_response = req.execute(schema, context).await;
        let gql_response = serde_json::to_string(&gql_batch_response)?;
        let mut response = match gql_batch_response.is_ok() {
            true => HttpResponse::Ok(),
            false => HttpResponse::BadRequest(),
        };
        Ok(response.content_type("application/json").body(gql_response))
    }

    /// Create a handler that replies with an HTML page containing GraphiQL. This does not handle routing, so you can mount it on any endpoint
    #[allow(dead_code)]
    pub async fn graphiql_handler(
        graphql_endpoint_url: &str,
        subscriptions_endpoint_url: Option<&'static str>,
    ) -> Result<HttpResponse, Error> {
        let html = graphiql_source(graphql_endpoint_url, subscriptions_endpoint_url);
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html))
    }

    /// Create a handler that replies with an HTML page containing GraphQL Playground. This does not handle routing, so you cant mount it on any endpoint.
    pub async fn playground_handler(
        graphql_endpoint_url: &str,
        subscriptions_endpoint_url: Option<&'static str>,
    ) -> Result<HttpResponse, Error> {
        let html = playground_source(graphql_endpoint_url, subscriptions_endpoint_url);
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(html))
    }
}
