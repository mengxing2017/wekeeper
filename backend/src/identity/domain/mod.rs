use crate::business::{Result, require};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
pub fn password_hash(password: &str) -> Result<String> {
    require((12..=128).contains(&password.len()), "密码须为 12–128 字节")?;
    let salt = SaltString::encode_b64(uuid::Uuid::new_v4().as_bytes()).expect("UUID salt");
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| crate::business::Error::Unavailable)?
        .to_string())
}
pub fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).ok().is_some_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}
