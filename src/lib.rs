// #HACK this brings juniper into crate level global scope because apparently
// juniper's macros expect the base juniper crate to be at root instead of importing juniper from
// itself?
// Juniper users will need to do
// ```
// #[macro_use]
// extern crate atb as juniper
// ```
// In the end, macro heavy crates may not be suitable for this kind of library aggregation.  We
// currently do not use any macros for sqlx but if they also don't have sanitized use statements,
// then we would be SOL
#[cfg(feature = "graphql")]
pub use juniper::*;

pub mod logging;
pub mod types;

#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(feature = "jwt")]
pub mod jwt;
#[cfg(feature = "sql")]
pub mod sql;

pub mod includes {
    // non-feature gated libs
    pub use anyhow;
    pub use chrono;
    pub use futures;
    pub use lazy_static;
    pub use log;
    pub use thiserror;
    pub use uuid;
}

#[cfg(feature = "http")]
pub mod http {
    pub use actix;
    pub use actix_cors;
    pub use actix_web;
    pub use actix_web_httpauth;
}

#[cfg(feature = "graphql")]
pub mod graphql;

#[cfg(feature = "eventsourcing")]
pub mod eventsourcing {
    pub use lucidstream;
    pub use lucidstream_ges;
}

pub mod prelude {
    pub use crate::logging;
    pub use crate::types::*;

    #[cfg(feature = "http")]
    pub use crate::http::*;

    #[cfg(feature = "graphql")]
    pub use crate::graphql::*;

    #[cfg(feature = "jwt")]
    pub use crate::jwt;

    #[cfg(feature = "sql")]
    pub use crate::sql::*;

    #[cfg(feature = "eventsourcing")]
    pub use crate::eventsourcing::*;

    pub use crate::includes::*;
}

///#HACK juniper actix isn't updated for beta channel actix for Tokio 1.0
/// https://github.com/aetheras-io/atb-rs/issues/1
#[cfg(feature = "graphql")]
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
    ///
    /// For example:
    ///
    /// ```
    /// # use juniper_actix::graphiql_handler;
    /// # use actix_web::{web, App};
    ///
    /// let app = App::new()
    ///          .route("/", web::get().to(|| graphiql_handler("/graphql", Some("/graphql/subscriptions"))));
    /// ```
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
