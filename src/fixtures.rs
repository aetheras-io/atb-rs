use lazy_static::lazy_static;
use std::str::FromStr;
use uuid::Uuid;

pub use fake;

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
pub mod jwt {
    use super::*;
    use jsonwebtoken::{DecodingKey, EncodingKey};

    pub static PRIV_KEY_BYTES: &[u8] = include_bytes!("../fixtures/test_priv.pem");
    pub static PUB_KEY_BYTES: &[u8] = include_bytes!("../fixtures/test_pub.pem");

    lazy_static! {
        pub static ref JWT_ENCODING_KEY: EncodingKey =
            EncodingKey::from_rsa_pem(PRIV_KEY_BYTES).expect("priv pem format is correct. qed");
        pub static ref JWT_DECODING_KEY: DecodingKey =
            DecodingKey::from_rsa_pem(PUB_KEY_BYTES).expect("pub pem format is correct. qed");
    }
}
