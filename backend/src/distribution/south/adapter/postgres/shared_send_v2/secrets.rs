//! One configured root key: existing envelope encryption and domain-separated grant signing.
use super::*;
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
fn root_key() -> Result<[u8; 32]> {
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
    Ok(bytes)
}
fn key() -> Result<aead::LessSafeKey> {
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, &root_key()?).map_err(|_| Error::Unavailable)?,
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
        aead::Aad::from(b"shared_mobile_send/2"),
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
            aead::Aad::from(b"shared_mobile_send/2"),
            &mut body,
        )
        .map_err(|_| Error::Policy("shared_send_secret_invalid"))?;
    serde_json::from_slice(plaintext).map_err(|_| Error::Conflict)
}
pub(super) fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// HKDF separates the signing scalar from the existing at-rest key. Stable across instances/restarts.
fn signing_key(root: &[u8; 32]) -> Result<aws_lc_rs::signature::EcdsaKeyPair> {
    use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair};
    use ring::hkdf;
    let salt = hkdf::Salt::new(
        hkdf::HKDF_SHA256,
        b"wekeeper/shared-send/execution-signing/v1",
    );
    let prk = salt.extract(root);
    // Rejection sampling: a derived 256-bit value is very rarely outside P-256's scalar range.
    for counter in 0u32..256 {
        let counter = counter.to_be_bytes();
        let info = [b"p256-private-key".as_slice(), counter.as_slice()];
        let mut scalar = [0u8; 32];
        prk.expand(&info, hkdf::HKDF_SHA256)
            .map_err(|_| Error::Unavailable)?
            .fill(&mut scalar)
            .map_err(|_| Error::Unavailable)?;
        // RFC 5915 ECPrivateKey, named curve prime256v1; the library derives and validates Q.
        let mut der = vec![0x30, 0x31, 0x02, 0x01, 0x01, 0x04, 0x20];
        der.extend_from_slice(&scalar);
        der.extend_from_slice(&[
            0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
        ]);
        let pair = EcdsaKeyPair::from_private_key_der(&ECDSA_P256_SHA256_ASN1_SIGNING, &der);
        scalar.fill(0);
        der.fill(0);
        if let Ok(pair) = pair {
            return Ok(pair);
        }
    }
    Err(Error::Unavailable)
}
pub(super) fn sign(value: &Value) -> Result<(String, String)> {
    sign_with(value, &root_key()?)
}
fn sign_with(value: &Value, root: &[u8; 32]) -> Result<(String, String)> {
    use aws_lc_rs::{rand::SystemRandom, signature::KeyPair};
    let pair = signing_key(root)?;
    let signature = pair
        .sign(&SystemRandom::new(), canonical(value)?.as_bytes())
        .map_err(|_| Error::Unavailable)?;
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    Ok((hex(pair.public_key().as_ref()), hex(signature.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_key(byte: u8) -> aead::LessSafeKey {
        aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &[byte; 32]).unwrap())
    }
    #[test]
    fn one_root_derives_stable_signing_identity_and_ring_verifiable_grants() {
        use aws_lc_rs::signature::KeyPair;
        use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
        let root = [7u8; 32];
        let first = signing_key(&root).unwrap();
        let again = signing_key(&root).unwrap();
        let other = signing_key(&[8u8; 32]).unwrap();
        assert_eq!(first.public_key().as_ref(), again.public_key().as_ref());
        assert_ne!(first.public_key().as_ref(), other.public_key().as_ref());
        let body = json!({"execution_id":"fixture", "message":"通知🙂", "max_calls":1});
        let (public, signature) = sign_with(&body, &root).unwrap();
        let decode = |s: &str| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>()
        };
        let signature = decode(&signature);
        assert_eq!(decode(&public), first.public_key().as_ref());
        let verifier = UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, first.public_key().as_ref());
        verifier
            .verify(canonical(&body).unwrap().as_bytes(), &signature)
            .unwrap();
        assert!(verifier.verify(b"changed", &signature).is_err());
        assert!(
            UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, other.public_key().as_ref())
                .verify(canonical(&body).unwrap().as_bytes(), &signature)
                .is_err()
        );
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
