//! Collection of commonly used types
#[cfg(feature = "jwt")]
pub use crate::jwt::Claims;

pub use chrono::{Duration, Utc};
pub use uuid::Uuid;

pub type DateTime = chrono::DateTime<Utc>;
