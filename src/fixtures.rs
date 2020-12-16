use std::env;
use std::fs::{self, File};
use std::io::Read;

use lazy_static::lazy_static;
use std::str::FromStr;
use uuid::Uuid;

pub use fake;

#[cfg(feature = "jwt")]
pub use jwt::*;

lazy_static! {
    // test users
    pub static ref ALICE: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000001").expect("uuid string is valid. qed");
    pub static ref BOB: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000002").expect("uuid string is valid. qed");
    pub static ref CHARLIE: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000003").expect("uuid string is valid. qed");
    pub static ref DAVE: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000004").expect("uuid string is valid. qed");
    pub static ref EVE: Uuid = Uuid::from_str("00000000-0000-0000-0000-000000000005").expect("uuid string is valid. qed");

    // test admins
    pub static ref ADMIN_A: Uuid = Uuid::from_str("000000AD-0000-0000-0000-000000000001").expect("uuid string is valid. qed");
    pub static ref ADMIN_B: Uuid = Uuid::from_str("000000AD-0000-0000-0000-000000000002").expect("uuid string is valid. qed");
}

#[cfg(feature = "jwt")]
mod jwt {
    use super::*;
    use jsonwebtoken::{DecodingKey, EncodingKey};

    lazy_static! {
        pub static ref PRIV_KEY_BYTES: Vec<u8> = {
            let file = env::var("PRIV_KEY_PATH").unwrap_or("../fixtures/test_priv.pem".to_owned());
            let mut f = File::open(&file).expect("no file found");
            let metadata = fs::metadata(&file).expect("unable to read metadata");
            let mut buffer = vec![0; metadata.len() as usize];
            f.read(&mut buffer).expect("buffer overflow");
            buffer
        };
        pub static ref PUB_KEY_BYTES: Vec<u8> = {
            let file = env::var("PUB_KEY_PATH").unwrap_or("../fixtures/test_pub.pem".to_owned());
            let mut f = File::open(&file).expect("no file found");
            let metadata = fs::metadata(&file).expect("unable to read metadata");
            let mut buffer = vec![0; metadata.len() as usize];
            f.read(&mut buffer).expect("buffer overflow");
            buffer
        };
        pub static ref JWT_ENCODING_KEY: EncodingKey =
            EncodingKey::from_rsa_pem(&PRIV_KEY_BYTES).expect("priv pem format is correct. qed");
        pub static ref JWT_DECODING_KEY: DecodingKey<'static> =
            DecodingKey::from_rsa_pem(&PUB_KEY_BYTES).expect("pub pem format is correct. qed");
    }
}
