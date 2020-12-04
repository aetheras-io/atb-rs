pub mod logging;
pub mod types;

#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(feature = "jwt")]
pub mod jwt;

// non-feature gated libs
pub use anyhow;
pub use chrono;
pub use futures;
pub use lazy_static;
pub use log;
pub use thiserror;
pub use uuid;

#[cfg(feature = "http")]
pub mod http {
    pub use actix;
    pub use actix_cors;
    pub use actix_threadpool;
    pub use actix_web;
    pub use actix_web_httpauth;
}

#[cfg(feature = "graphql")]
pub mod graphql {
    pub use dataloader;
    pub use juniper;

    #[cfg(feature = "http")]
    pub use juniper_actix;
}

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
    pub use sqlx;

    pub use anyhow;
    pub use chrono;
    pub use futures;
    pub use lazy_static;
    pub use log;
    pub use thiserror;
    pub use uuid;
}
