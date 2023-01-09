use lazy_static::lazy_static;
use std::str::FromStr;

#[cfg(feature = "uuidv1")]
use uuidv1::Uuid;
#[cfg(feature = "uuidv0_8")]
use uuidv0_8::Uuid;

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
