//! Collection of commonly used types
#[cfg(feature = "jwt")]
pub use crate::jwt::Claims;

pub use chrono::{Duration, Utc};
pub use uuid::Uuid;

pub type DateTime = chrono::DateTime<Utc>;

pub struct Take<T>(Option<T>);

impl<T> Take<T> {
    pub fn new(item: T) -> Self {
        Self(Some(item))
    }
}

impl<T> std::ops::Deref for Take<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Some(t) => &t,
            None => panic!("value is already taken"),
        }
    }
}

impl<T> std::ops::DerefMut for Take<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match (*self).0 {
            Some(ref mut t) => t,
            None => panic!("value is already taken"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn take_can_be_mutated() {
        struct Container {
            data: Take<String>,
        }

        impl Container {
            fn mut_me(&mut self) {
                *self.data = self.create_the_data();
            }

            fn create_the_data(&mut self) -> String {
                //this will panic, and is exactly what we are trying to prevent
                *self.data = "something".to_owned();
                "hello".to_owned()
            }
        }

        let mut field = Container {
            data: Take::new("hello".to_owned()),
        };

        let f = &mut field;
        let inner = &f.data;
        *f.data = "world".to_owned() + inner;
    }
}
