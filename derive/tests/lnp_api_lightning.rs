#[macro_use]
extern crate inet2_derive;

use internet2::{CreateUnmarshaller, TypedEnum, Unmarshall};
use std::str::FromStr;

#[derive(Clone, PartialEq, Eq, Debug, LnpApi)]
#[lnp_api(encoding = "lightning")]
#[non_exhaustive]
pub enum Request {
    #[lnp_api(type = 0x0001)]
    Hello(String),

    /// Some attribute
    #[lnp_api(type = 0x0003)]
    Empty(),

    #[lnp_api(type = 0x0005)]
    NoArgs,

    #[lnp_api(type = 0x0103)]
    AddKeys(Vec<bitcoin::PublicKey>),
}

#[test]
fn roundtrip() {
    let unmarshaller = Request::create_unmarshaller();

    let message = Request::Hello("world".to_owned());
    let payload = message.serialize();
    assert_eq!(payload, b"\x00\x01\x05world".to_vec());
    let roundtrip = &*unmarshaller.unmarshall(&payload).unwrap();
    assert_eq!(&message, roundtrip);

    let message = Request::Empty();
    let payload = message.serialize();
    assert_eq!(payload, b"\x00\x03".to_vec());
    let roundtrip = &*unmarshaller.unmarshall(&payload).unwrap();
    assert_eq!(&message, roundtrip);

    let message = Request::NoArgs;
    let payload = message.serialize();
    assert_eq!(payload, b"\x00\x05".to_vec());
    let roundtrip = &*unmarshaller.unmarshall(&payload).unwrap();
    assert_eq!(&message, roundtrip);

    let keys: Vec<_> = vec![
        "020388ac0ff72e76002f6bdf1a08638390f0c43125c33688ca9e64cadff86248a6",
        "03c038e7a5a2710b50afe059c98085ce20455d7d5e681d5962b29e0a6727cfd9d4",
    ]
    .into_iter()
    .map(bitcoin::PublicKey::from_str)
    .map(Result::unwrap)
    .collect();
    let message = Request::AddKeys(keys.clone());
    let payload = message.serialize();
    let mut expect = b"\x01\x03\x02".to_vec();
    expect.extend(keys.iter().map(bitcoin::PublicKey::to_bytes).flatten());
    assert_eq!(payload, expect);
    let roundtrip = &*unmarshaller.unmarshall(&payload).unwrap();
    assert_eq!(&message, roundtrip);
}
