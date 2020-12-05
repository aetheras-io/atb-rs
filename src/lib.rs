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
    pub use actix_threadpool;
    pub use actix_web;
    pub use actix_web_httpauth;
}

#[cfg(feature = "graphql")]
pub mod graphql;

#[cfg(feature = "sql")]
pub mod sql {
    pub use sqlx;
}

#[cfg(feature = "eventsourcing")]
pub mod eventsourcing {
    pub use lucidstream;
    pub use lucidstream_ges;
    pub use lucidstream_inmemory;
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

    pub use crate::includes::*;
}
