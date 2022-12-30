pub mod helpers;
pub mod logging;

pub mod includes {
    pub use anyhow;
    pub use futures;
    pub use log;
    pub use thiserror;
}

#[cfg(feature = "eventsourcing")]
pub mod eventsourcing {
    pub use lucidstream;
    pub use lucidstream_ges;
}

pub mod prelude {
    pub use crate::logging;

    #[cfg(feature = "eventsourcing")]
    pub use crate::eventsourcing::*;

    pub use crate::includes::*;
}
