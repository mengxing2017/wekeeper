const PREFIX: &str = "WEKEEPER";
use super::domain::{Fault, Result, decode, encode, ensure};
use ring::{
    aead,
    rand::{SecureRandom, SystemRandom},
};
fn master_key_bytes(value: Option<&str>) -> Result<[u8; 32]> {
    let value = value
        .filter(|v| !v.is_empty())
        .ok_or(Fault("source_key_configuration_required"))?;
    let bytes = decode(value).map_err(|_| Fault("source_key_configuration_required"))?;
    bytes
        .try_into()
        .map_err(|_| Fault("source_key_configuration_required"))
}
fn key() -> Result<aead::LessSafeKey> {
    let variable = format!("{PREFIX}_SOURCE_MASTER_KEY_V1");
    let value = std::env::var(&variable).ok();
    let bytes = master_key_bytes(value.as_deref()).inspect_err(|_| {
        tracing::error!(variable, "source credential encryption requires a stable 32-byte base64url master key; value omitted");
    })?;
    Ok(aead::LessSafeKey::new(
        aead::UnboundKey::new(&aead::AES_256_GCM, &bytes)
            .map_err(|_| Fault("temporarily_unavailable"))?,
    ))
}
pub fn random_secret() -> Result<String> {
    let mut b = [0; 32];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| Fault("temporarily_unavailable"))?;
    Ok(encode(&b))
}
pub fn seal(value: &str, context: &str) -> Result<Vec<u8>> {
    let mut nonce = [0; 12];
    SystemRandom::new()
        .fill(&mut nonce)
        .map_err(|_| Fault("temporarily_unavailable"))?;
    let mut body = value.as_bytes().to_vec();
    key()?
        .seal_in_place_append_tag(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context.as_bytes()),
            &mut body,
        )
        .map_err(|_| Fault("temporarily_unavailable"))?;
    Ok([nonce.to_vec(), body].concat())
}
pub fn open(bytes: &[u8], context: &str) -> Result<String> {
    ensure(bytes.len() >= 28, "temporarily_unavailable")?;
    let mut body = bytes[12..].to_vec();
    let nonce: [u8; 12] = bytes[..12]
        .try_into()
        .map_err(|_| Fault("temporarily_unavailable"))?;
    let plain = key()?
        .open_in_place(
            aead::Nonce::assume_unique_for_key(nonce),
            aead::Aad::from(context.as_bytes()),
            &mut body,
        )
        .map_err(|_| Fault("temporarily_unavailable"))?;
    String::from_utf8(plain.to_vec()).map_err(|_| Fault("temporarily_unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_invalid_master_key_is_a_configuration_error() {
        for value in [None, Some(""), Some("not base64url!"), Some("c2hvcnQ")] {
            assert_eq!(
                master_key_bytes(value).unwrap_err().0,
                "source_key_configuration_required"
            );
        }
        let encoded = encode(&[42; 32]);
        assert_eq!(master_key_bytes(Some(&encoded)).unwrap(), [42; 32]);
    }
}
