pub trait Cursor {
    fn cursor(&self) -> String;
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
