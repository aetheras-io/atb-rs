//! Juniper Graphql Helpers.  #NOTE currently juniper is a little problematic to use as a reexport
//! because of some macro use statement problems when importing juniper from another crate.  I
//! don't think their macros were written/sanitized for this use case.  The big issue later down
//! the road is keeping juniper versions in sync, since the relay spec is defined here.  This will
//! be pretty difficult to debug but keep an eye out on it and if there is time, we can take a look
//! at their proc macros to see how our use case can be supported.
pub use dataloader;

#[cfg(feature = "http")]
pub use juniper_actix;

pub mod spec {
    use std::io::Cursor;

    use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
    use juniper::GraphQLObject;

    #[derive(thiserror::Error, Debug)]
    pub enum CursorError {
        #[error("Cursor has invalid length")]
        InvalidLength,

        #[error("Io error: {0}")]
        Io(#[from] std::io::Error),

        #[error("Base64 decode error: {0}")]
        Base64Decode(#[from] base64::DecodeError),
    }

    /// Encode an 'opaque' cursor, recommended by the relay connections spec.
    /// it is recommended to use a auto-incrementing key plus an identifier, such as the
    /// table name.  This guarantees uniqueness across the same database
    pub fn encode_cursor(
        wtr: &mut Vec<u8>,
        index: i64,
        ident: &[u8],
    ) -> Result<String, CursorError> {
        wtr.write_i64::<BigEndian>(index)?;
        wtr.extend(ident);
        Ok(base64::encode(wtr))
    }

    /// Decode an 'opaque' cursor consisting of an `i64` prefix and arbitrary bytes
    pub fn decode_cursor(encoded: Vec<u8>) -> Result<(i64, Vec<u8>), CursorError> {
        if encoded.len() <= 8 {
            return Err(CursorError::InvalidLength);
        }

        let mut decoded = base64::decode(encoded)?;
        // i64 prefix is 8 bytes
        let rhs = decoded.split_off(8);
        let mut rdr = Cursor::new(decoded);
        Ok((rdr.read_i64::<BigEndian>()?, rhs))
    }

    /// Graphql Relay Connections PageInfo
    /// according to the "updated" implementation, "startCursor" and "endCursor"
    /// should be null when "Edges" is empty.  However, the graphql spec hasn't been updated yet
    /// https://github.com/rmosolgo/graphql-ruby/pull/2886
    #[derive(Default, GraphQLObject)]
    pub struct PageInfo {
        pub has_previous_page: bool,
        pub has_next_page: bool,
        pub start_cursor: Option<String>,
        pub end_cursor: Option<String>,
    }

    /// A trait that a Connections Node must implement in order to be used in connections
    pub trait IntoEdge<T> {
        fn index(&self) -> i64;

        fn index_field() -> &'static str;

        fn cursor_key() -> &'static str;

        fn into_edge(self, cursor: String) -> T;
    }

    /// A trait implemented by edges that returns its cursor
    pub trait EdgeCursor {
        fn cursor(&self) -> &str;
    }

    #[macro_export]
    macro_rules! impl_relay_connection {
        ($connection:ident, $edge:ident, $type:ty, $context:ty) => {
            /// Relay Connections spec'd `Edge`
            /// https://relay.dev/graphql/connections.htm
            #[derive(juniper::GraphQLObject)]
            #[graphql(context = $context)]
            pub struct $edge {
                pub node: $type,
                pub cursor: String,
            }

            impl $crate::graphql::spec::EdgeCursor for $edge {
                fn cursor(&self) -> &str {
                    &self.cursor
                }
            }

            /// Relay Connections spec'd `Connection`
            /// https://relay.dev/graphql/connections.htm
            #[derive(juniper::GraphQLObject)]
            #[graphql(context = $context)]
            pub struct $connection {
                pub edges: Vec<$edge>,
                pub page_info: $crate::graphql::spec::PageInfo,
            }
        };
    }

    #[cfg(test)]
    mod test {
        pub struct User;

        #[juniper::graphql_object(Context = Context)]
        impl User {
            pub fn id(&self) -> i32 {
                42
            }
        }

        pub struct Context;
        impl juniper::Context for Context {}

        impl_relay_connection!(UsersConnection, UserEdge, i32, ());

        //#TODO add some tests here
        #[test]
        fn it_can_encode_cursor() {}

        #[test]
        fn it_can_decode_cursor() {}
    }
}
