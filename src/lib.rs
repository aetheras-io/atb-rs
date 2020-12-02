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

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Clone, Default, Serialize, Deserialize)]
    pub struct Claims {
        #[serde(default)]
        pub iss: String,

        #[serde(default)]
        pub iat: i64,

        #[serde(default)]
        pub exp: i64,

        #[serde(default)]
        pub nbf: i64,

        #[serde(default)]
        pub sub: Uuid,

        #[serde(flatten)]
        pub custom: std::collections::HashMap<String, serde_json::Value>,
    }

    use super::prelude::*;
    use chrono::Utc;

    #[test]
    fn it_works() {
        let custom: std::collections::HashMap<String, serde_json::Value> =
            [("nbf".to_owned(), serde_json::to_value("hello").unwrap())]
                .iter()
                .cloned()
                .collect();

        let c = Claims {
            iss: "hello".to_owned(),
            iat: 3,
            exp: 4,
            nbf: 5,
            sub: Uuid::new_v4(),
            custom,
        };

        println!("{}", serde_json::to_string(&c).unwrap());

        let t: Claims = serde_json::from_str(
            r#"{"iss":"hello","iat":3,"exp":4,"nbf":5,"sub":"bf5d06e8-0e74-4e93-a41e-6e0a5615927b","nbf":"hello"}"#,
        ).unwrap();
        println!("{:?}", t);
    }
}
