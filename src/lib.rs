#[cfg(feature = "fixtures")]
pub mod fixtures;
#[cfg(feature = "jwt")]
pub mod jwt;
pub mod logging;

// non-feature gated libs
pub use anyhow;
pub use chrono;
pub use thiserror;
pub use uuid;

#[cfg(feature = "sql")]
pub use sqlx;

#[cfg(feature = "graphql")]
pub mod graphql {
    pub use dataloader;
    pub use juniper;

    #[cfg(feature = "http")]
    pub use juniper_actix;
}

mod prelude {
    #[cfg(feature = "graphql")]
    pub use crate::graphql::*;

    #[cfg(feature = "sql")]
    pub use sqlx;

    pub use anyhow;
    pub use chrono;
    pub use thiserror;
    pub use uuid::{self, Uuid};
}
