//! Separate at-rest key. Missing / malformed keys inhibit issuing sends, never fall back to plaintext.
use super::*;
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
fn key() -> Result<aead::LessSafeKey> {
    let value = std::env::var("WEKEEPER_SHARED_SEND_KEY")
        .map_err(|_| Error::Policy("shared_send_key_required"))?;
    require(
        value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "shared_send_key_required",
    )?;
    let mut bytes = [0u8; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| Error::Policy("shared_send_key_required"))?;
    }
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, &bytes).map_err(|_| Error::Unavailable)?,
    ))
}
pub(super) fn seal(value: &Value) -> Result<Vec<u8>> {
    seal_with(value, &key()?)
}
fn seal_with(value: &Value, key: &aead::LessSafeKey) -> Result<Vec<u8>> {
    let mut nonce = [0u8; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| Error::Unavailable)?;
    let mut body = serde_json::to_vec(value).map_err(|_| Error::Conflict)?;
    key.seal_in_place_append_tag(
        aead::Nonce::assume_unique_for_key(nonce),
        aead::Aad::from(b"shared_mobile_send/1"),
        &mut body,
    )
    .map_err(|_| Error::Unavailable)?;
    Ok([nonce.to_vec(), body].concat())
}
pub(super) fn open(value: &[u8]) -> Result<Value> {
    open_with(value, &key()?)
}
fn open_with(value: &[u8], key: &aead::LessSafeKey) -> Result<Value> {
    require(value.len() >= 28, "shared_send_secret_invalid")?;
    let nonce = <[u8; 12]>::try_from(&value[..12]).map_err(|_| Error::Conflict)?;
    let mut body = value[12..].to_vec();
    let plaintext = key
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(b"shared_mobile_send/1"),
            &mut body,
        )
        .map_err(|_| Error::Policy("shared_send_secret_invalid"))?;
    serde_json::from_slice(plaintext).map_err(|_| Error::Conflict)
}
pub(super) fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_key(byte: u8) -> aead::LessSafeKey {
        aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &[byte; 32]).unwrap())
    }
    #[test]
    fn secret_storage_authenticates_and_randomizes_every_response() {
        let value =
            json!({"result_credential":"only-native","invocation_secret":"never-plaintext"});
        let a = seal_with(&value, &test_key(1)).unwrap();
        let b = seal_with(&value, &test_key(1)).unwrap();
        assert_ne!(a, b);
        assert_eq!(open_with(&a, &test_key(1)).unwrap(), value);
        assert!(!String::from_utf8_lossy(&a).contains("only-native"));
        assert!(open_with(&a, &test_key(2)).is_err());
        let mut tampered = a;
        tampered[15] ^= 1;
        assert!(open_with(&tampered, &test_key(1)).is_err());
    }
}
