pub use quaint;
pub use sqlx;

use quaint::Value;
use sqlx::database::HasArguments;
use sqlx::query::Query;

/// Helper type for pg_bind_value function
type PgArgs<'a> = <sqlx::Postgres as HasArguments<'a>>::Arguments;

/// This is a quaint value to sqlx bind helper. Arrays and Chars are not supported.
/// It will be a little tricky to unpack Value::Array
pub fn pg_bind_value<'a>(
    value: Value,
    query: Query<'a, sqlx::Postgres, PgArgs>,
) -> Query<'a, sqlx::Postgres, PgArgs<'a>> {
    match value {
        Value::Integer(v) => query.bind(v),
        Value::Text(v) => query.bind(v.map(|cow| cow.into_owned())),
        Value::Float(v) => query.bind(v),
        Value::Double(v) => query.bind(v),
        Value::Enum(v) => query.bind(v.map(|cow| cow.into_owned())),
        Value::Bytes(v) => query.bind(v.map(|cow| cow.into_owned())),
        Value::Boolean(v) => query.bind(v),
        Value::Json(v) => query.bind(v),
        Value::Uuid(v) => query.bind(v),
        Value::DateTime(v) => query.bind(v),
        Value::Numeric(v) => query.bind(v),
        // Value::Char(v) => query.bind(v),
        // Value::Array(v) => query.bind(v.map(|arr| {
        //     arr.into_iter()
        //         .map(|vv| match vv {
        //             Value::Integer(v) => v,
        //         })
        //         .collect()
        // })),
        _ => panic!("unsupported quaint value conversion"),
    }
}
