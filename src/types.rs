//! Collection of commonly used types

pub use chrono::{Duration, Utc};
pub use uuid::Uuid;

pub type DateTime = chrono::DateTime<Utc>;
