#[cfg(feature = "jwt")]
pub mod jwt;

#[cfg(feature = "fixtures")]
pub mod fixtures;

pub mod prelude {
    pub use chrono;
    pub use uuid;

    pub use super::*;

    #[cfg(feature = "jwt")]
    pub use jwt::*;
}

pub use chrono::{Duration, Utc};
pub use uuid::Uuid;

pub type DateTime = chrono::DateTime<Utc>;

#[derive(Debug)]
pub struct Take<T>(Option<T>);

impl<T> Take<T> {
    pub fn new(item: T) -> Self {
        Self(Some(item))
    }

    pub fn take(&mut self) -> Option<T> {
        std::mem::take(&mut self.0)
    }

    pub fn insert(&mut self, item: T) -> &mut T {
        self.0.insert(item)
    }
}

impl<T> std::ops::Deref for Take<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Some(t) => t,
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
    use std::ops::Deref;

    #[derive(Debug)]
    struct Container {
        data: Take<String>,
    }

    impl Container {
        fn take_and_overwrite(&mut self) {
            let _ = self.data.take();
            self.overwrite();
        }

        fn overwrite(&mut self) {
            //this will panic, and is exactly what we are trying to prevent
            *self.data = "something".to_owned();
        }
    }

    #[test]
    fn it_can_be_mutated() {
        let mut field = Container {
            data: Take::new("hello".to_owned()),
        };

        let f = &mut field;
        let inner = &f.data;
        *f.data = [inner, " ", "world"].concat();

        assert_eq!(f.data.deref(), &"hello world".to_owned());
    }

    #[test]
    #[should_panic]
    fn it_should_panic_on_double_take() {
        let mut field = Container {
            data: Take::new("hello".to_owned()),
        };

        field.take_and_overwrite();
    }
}
