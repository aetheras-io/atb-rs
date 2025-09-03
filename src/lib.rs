pub mod helpers;
pub mod logging;

pub mod includes {
    pub use anyhow;
    pub use futures;
    pub use log;
    pub use once_cell;
    pub use thiserror;
    pub use tracing;
}

#[cfg(feature = "fixtures")]
pub use atb_fixtures_utils as fixtures;

#[cfg(feature = "eventsourcing")]
pub mod eventsourcing {
    pub use lucidstream;
    pub use lucidstream_pg;
}

pub mod prelude {
    pub use crate::logging;

    #[cfg(feature = "eventsourcing")]
    pub use crate::eventsourcing::*;

    pub use crate::includes::*;
}
