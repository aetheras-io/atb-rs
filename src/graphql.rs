//! Juniper Graphql Helpers.  #NOTE currently juniper is a little problematic to use as a reexport
//! because of some macro use statement problems when importing juniper from another crate.  I
//! don't think their macros were written/sanitized for this use case.  The big issue later down
//! the road is keeping juniper versions in sync, since the relay spec is defined here.  This will
//! be pretty difficult to debug but keep an eye out on it and if there is time, we can take a look
//! at their proc macros to see how our use case can be supported.
pub use dataloader;

#[cfg(feature = "http")]
pub use crate::juniper_actix;
// pub use juniper_actix;

#[cfg(feature = "sql")]
pub mod connections {
    use super::spec::*;
    use crate::sql;
    use std::convert::Into;

    use quaint::prelude::*;
    use quaint::visitor::{Postgres, Visitor};
    use sqlx::{
        postgres::{PgPool, PgRow},
        Row,
    };

    #[derive(thiserror::Error, Debug)]
    pub enum Error {
        #[error("Invalid connections query")]
        InvalidQuery,

        #[error("Custom column options cannot be empty")]
        InvalidColumnOpts,

        #[error("Connection spec cursor error: {0}")]
        Cursor(#[from] CursorError),

        #[error("Quaint error: {0}")]
        Quaint(#[from] quaint::error::Error),

        #[error("Sqlx error: {0}")]
        Sqlx(#[from] sqlx::Error),
    }

    pub type Result<T, E = Error> = std::result::Result<T, E>;

    pub enum ColumnOpts<'a, S> {
        All,
        Custom(&'a [S]),
    }

    impl<'a, S> From<&'a [S]> for ColumnOpts<'a, S> {
        fn from(other: &'a [S]) -> Self {
            ColumnOpts::Custom(other)
        }
    }

    /// Generic Connections Helper following graphql relay spec
    pub async fn execute<'a, S, C, T, E, F>(
        pg_pool: &PgPool,
        table: &'a str,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
        conditions: Option<C>,
        column_opts: ColumnOpts<'a, S>,
        map_row: F,
    ) -> Result<(Vec<E>, PageInfo)>
    where
        S: AsRef<str>,
        C: Into<ConditionTree<'a>> + Clone,
        T: IntoEdge<E>,
        E: Edge<T>,
        F: Fn(PgRow) -> T,
    {
        let query_meta = build_raw::<S, C>(
            table,
            T::index_field(),
            first,
            after,
            last,
            before,
            column_opts,
            conditions,
        )?;

        log::debug!("Connections Query: \n {}", query_meta.sql_statement);
        log::debug!("Connections Params: \n {:?}", query_meta.parameters);
        query_raw::<T, E, F>(pg_pool, table, query_meta, map_row).await
    }

    /// Raw Query container struct
    pub struct QueryMeta<'a> {
        sql_statement: String,
        parameters: Vec<Value<'a>>,
        fetch_count: usize,
        ascending: bool,
        parse_previous: bool,
    }

    // #TODO
    // handle custom order_by.  This is very tricky because if the custom order_by fields allow
    // duplicates, then the previous_count query becomes way more complicated.  May not be worth
    // the complexity right now.
    /// Build a raw connections query based on the graphql spec parameters and custom conditions
    pub fn build_raw<'a, S, C>(
        table: &'a str,
        index_field: &'a str,
        first: Option<i32>,
        after: Option<String>,
        last: Option<i32>,
        before: Option<String>,
        column_opts: ColumnOpts<'a, S>,
        conditions: Option<C>,
    ) -> Result<QueryMeta<'a>>
    where
        S: AsRef<str>,
        C: Into<ConditionTree<'a>> + Clone,
    {
        let (over_fetch_count, ascending, maybe_cursor) = match (&first, &last) {
            (Some(count), None) => Ok((*count as usize + 1, true, after)),
            (None, Some(count)) => Ok((*count as usize + 1, false, before)),
            (None, None) => Ok((5, true, None)),
            _ => Err(Error::InvalidQuery),
        }?;

        let maybe_cursor = maybe_cursor
            .map(|c| decode_cursor(c.into_bytes(), table.as_bytes()))
            .transpose()?;

        let parse_previous = maybe_cursor.is_some();
        let base_query = match column_opts {
            ColumnOpts::All => Select::from_table(table).value(Table::from(table).asterisk()),
            ColumnOpts::Custom(columns) => {
                if columns.is_empty() {
                    return Err(Error::InvalidColumnOpts);
                }
                columns
                    .iter()
                    .fold(Select::from_table(table), |select_table, c| {
                        select_table.column(c.as_ref())
                    })
            }
        }
        .limit(over_fetch_count);

        let (order, maybe_cursor) = match (ascending, maybe_cursor) {
            (true, Some(index)) => (
                index_field.ascend(),
                Some((
                    index_field.less_than(index),
                    index_field.greater_than(index),
                )),
            ),
            (true, None) => (index_field.ascend(), None),
            (false, Some(index)) => (
                index_field.descend(),
                Some((
                    index_field.greater_than(index),
                    index_field.less_than(index),
                )),
            ),
            (false, None) => (index_field.descend(), None),
        };

        let query = maybe_cursor
            .into_iter()
            .fold(base_query, |bq, (inner, outer)| {
                bq.value(
                    Select::from_table(table)
                        .value(count(asterisk()).alias("previous_count"))
                        .so_that(match conditions.clone() {
                            Some(c) => inner.and(c.into()),
                            None => inner.into(),
                        }),
                )
                .so_that(outer)
            })
            .order_by(order);
        let final_query = conditions.into_iter().fold(query, |q, c| q.and_where(c));
        let (sql_statement, parameters) = Postgres::build(final_query)?;

        Ok(QueryMeta {
            sql_statement,
            parameters,
            fetch_count: over_fetch_count,
            ascending,
            parse_previous,
        })
    }

    /// Execute a raw connections query generated by `query_raw`
    pub async fn query_raw<'a, T, E, F>(
        pg_pool: &PgPool,
        table: &'a str,
        meta: QueryMeta<'a>,
        map_row: F,
    ) -> Result<(Vec<E>, PageInfo)>
    where
        T: IntoEdge<E>,
        E: Edge<T>,
        F: Fn(PgRow) -> T,
    {
        let QueryMeta {
            sql_statement,
            parameters,
            fetch_count,
            ascending,
            parse_previous,
        } = meta;

        let mut rows = parameters
            .into_iter()
            .fold(sqlx::query(&sql_statement), |q, key| {
                sql::pg_bind_value(key, q)
            })
            .fetch_all(pg_pool)
            .await?;

        let row_count = rows.len();
        if row_count >= fetch_count {
            let _ = rows.pop();
        }

        let mut buf = vec![];
        let mut has_previous: Option<bool> = None;
        let edges = rows
            .into_iter()
            .map(|row: PgRow| {
                if parse_previous && has_previous.is_none() {
                    // If we have a cursor, we created the "previous_count" as a subquery appended to
                    // the end of the columns
                    has_previous = Some(row.get::<i64, _>(row.len() - 1) > 0);
                }
                let item: T = map_row(row);
                let cursor = encode_cursor(&mut buf, item.index(), table.as_bytes())?;
                let edge = item.into_edge(cursor);
                buf.clear();
                Ok(edge)
            })
            .collect::<Result<Vec<E>>>()?;

        let (edges, has_previous_page, has_next_page) = if ascending {
            (
                edges,
                has_previous.unwrap_or(false),
                row_count == fetch_count,
            )
        } else {
            (
                edges.into_iter().rev().collect(),
                row_count == fetch_count,
                has_previous.unwrap_or(false),
            )
        };

        let start_cursor = edges.first().map(|e| e.cursor().to_owned());
        let end_cursor = edges.last().map(|e| e.cursor().to_owned());
        Ok((
            edges,
            PageInfo {
                start_cursor,
                end_cursor,
                has_previous_page,
                has_next_page,
            },
        ))
    }
}

pub mod spec {
    use std::io::Cursor;

    use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
    use juniper::GraphQLObject;

    #[derive(thiserror::Error, Debug)]
    pub enum CursorError {
        #[error("Cursor has invalid length")]
        InvalidLength,

        #[error("Decoded cursor identity mismatch")]
        IdentityMismatch,

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
    pub fn decode_cursor(encoded: Vec<u8>, ident: &[u8]) -> Result<i64, CursorError> {
        if encoded.len() <= 8 {
            return Err(CursorError::InvalidLength);
        }

        let mut decoded = base64::decode(encoded)?;
        // i64 prefix is 8 bytes
        let rhs = decoded.split_off(8);
        let mut rdr = Cursor::new(decoded);
        if ident != rhs {
            Err(CursorError::IdentityMismatch)
        } else {
            rdr.read_i64::<BigEndian>().map_err(Into::into)
        }
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
        /// Field is used to create the cursor
        fn index(&self) -> i64;

        /// Field is used for connection query logic
        fn index_field() -> &'static str;

        /// Conversion method, letting the caller create the cursor (this is an optimization)
        fn into_edge(self, cursor: String) -> T;
    }

    /// An abstract concept of an Edge in graphql connections
    pub trait Edge<T> {
        fn node(&self) -> &T;
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

            impl $crate::graphql::spec::Edge<$type> for $edge {
                fn node(&self) -> &$type {
                    &self.node
                }

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
        use super::*;

        pub struct User {
            user_number: i64,
        }

        #[juniper::graphql_object(Context = Context)]
        impl User {
            pub fn id(&self) -> i32 {
                42
            }
        }

        pub struct Context;
        impl juniper::Context for Context {}

        impl_relay_connection!(UsersConnection, UserEdge, User, Context);

        impl IntoEdge<UserEdge> for User {
            fn index(&self) -> i64 {
                self.user_number
            }

            fn index_field() -> &'static str {
                "user_number"
            }

            fn into_edge(self, cursor: String) -> UserEdge {
                UserEdge { node: self, cursor }
            }
        }

        //#TODO add some tests here
        #[test]
        fn it_can_encode_cursor() {}

        #[test]
        fn it_can_decode_cursor() {}
    }
}
