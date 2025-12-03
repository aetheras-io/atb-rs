use async_graphql::{Error, Result, connection::PageInfo};

pub trait Cursor {
    fn cursor(&self) -> String;
}

const MAX_PAGE_SIZE: usize = 200;

pub fn validate_connection<T>(
    after: Option<&T>,
    before: Option<&T>,
    first: Option<i32>,
    last: Option<i32>,
    max_page_size: Option<usize>,
) -> Result<(bool, usize)> {
    let max_page_size = max_page_size.unwrap_or(MAX_PAGE_SIZE);

    if first.is_some() && last.is_some() {
        return Err("The \"first\" and \"last\" parameters cannot exist at the same time".into());
    }

    let first = match first {
        Some(first) if first < 0 => {
            return Err("The \"first\" parameter must be a non-negative number".into());
        }
        Some(first) if first as usize > max_page_size => {
            return Err(format!("The \"first\" parameter must be at most {max_page_size}").into());
        }
        Some(first) => Some(first as usize),
        None => None,
    };

    let last = match last {
        Some(last) if last < 0 => {
            return Err("The \"last\" parameter must be a non-negative number".into());
        }
        Some(last) if last as usize > max_page_size => {
            return Err(format!("The \"last\" parameter must be at most {max_page_size}").into());
        }
        Some(last) => Some(last as usize),
        None => None,
    };

    match (&after, &before, first, last) {
        (_, None, Some(first), None) => Ok((true, first)),
        (None, _, None, Some(last)) => Ok((false, last)),
        _ => Err(Error::new("Invalid pagination parameters")),
    }
}

pub fn finalize_page_info<T>(
    data: &mut Vec<T>,
    count: usize,
    is_ascending: bool,
    has_after: bool,
    has_before: bool,
) -> PageInfo
where
    T: Cursor,
{
    let has_more = data.len() > count;
    if has_more {
        // If we fetched an extra record, remove it now
        data.pop();
    }
    let (has_previous_page, has_next_page) = if is_ascending {
        (has_after, has_more)
    } else {
        data.reverse();
        (has_more, has_before)
    };

    PageInfo {
        start_cursor: data.first().map(|e| e.cursor()),
        end_cursor: data.last().map(|e| e.cursor()),
        has_previous_page,
        has_next_page,
    }
}

pub use atb_graphql_derive::*;

//#HACK for macro expansion tests
#[cfg(test)]
extern crate self as atb_graphql_ext;

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::SimpleObject;

    #[allow(unused)]
    #[derive(SimpleObject, GraphQLConnection, GraphQLIndexed)]
    pub struct TestObject {
        #[cursor]
        pub id: String,
    }

    #[test]
    fn macro_paths_compile() {
        // Nothing to run; the test passes if the module compiled.
    }
}
