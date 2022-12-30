use std::str::FromStr;

use once_cell::sync::Lazy;
use uuid::Uuid;

pub use fake;

// test users
pub static ALICE: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("00000000-0000-0000-0000-000000000001").expect("uuid string is valid. qed")
});
pub static BOB: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("00000000-0000-0000-0000-000000000002").expect("uuid string is valid. qed")
});
pub static CHARLIE: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("00000000-0000-0000-0000-000000000003").expect("uuid string is valid. qed")
});
pub static DAVE: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("00000000-0000-0000-0000-000000000004").expect("uuid string is valid. qed")
});
pub static EVE: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("00000000-0000-0000-0000-000000000005").expect("uuid string is valid. qed")
});

// test admins
pub static ADMIN_A: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("000000AD-0000-0000-0000-000000000001").expect("uuid string is valid. qed")
});
pub static ADMIN_B: Lazy<Uuid> = Lazy::new(|| {
    Uuid::from_str("000000AD-0000-0000-0000-000000000002").expect("uuid string is valid. qed")
});

#[cfg(feature = "jwt")]
pub mod jwt {
    use jsonwebtoken::{DecodingKey, EncodingKey};
    use once_cell::sync::Lazy;

    pub static PRIV_KEY_BYTES: &[u8] = include_bytes!("../res/test_priv.pem");
    pub static PUB_KEY_BYTES: &[u8] = include_bytes!("../res/test_pub.pem");

    pub static JWT_ENCODING_KEY: Lazy<EncodingKey> = Lazy::new(|| {
        EncodingKey::from_rsa_pem(PRIV_KEY_BYTES).expect("priv pem format is correct. qed")
    });
    pub static JWT_DECODING_KEY: Lazy<DecodingKey> = Lazy::new(|| {
        DecodingKey::from_rsa_pem(PUB_KEY_BYTES).expect("pub pem format is correct. qed")
    });
}
