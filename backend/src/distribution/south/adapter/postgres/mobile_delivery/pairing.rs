//! Human-entered codes; attempt counters commit independently of failed enrollment.
use super::*;

fn invalid() -> Error {
    Error::Policy("pairing_code_invalid")
}

fn random_code() -> String {
    loop {
        // The low 32 bits of a v4 UUID are fully random. Rejection avoids modulo bias.
        let sample = Uuid::new_v4().as_u128() as u32;
        if sample < 4_200_000_000 {
            return format!("{:08}", sample % 100_000_000);
        }
    }
}

pub(super) async fn issue(tx: &mut Tx<'_>, device: Uuid) -> Result<Value> {
    for _ in 0..32 {
        let code = random_code();
        let enrollment = Uuid::now_v7();
        let expires: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "INSERT INTO device_enrollments(id,device_id,code_hash,expires_at)
             VALUES($1,$2,$3,clock_timestamp()+interval '10 minutes')
             ON CONFLICT(code_hash) DO NOTHING RETURNING expires_at",
        )
        .bind(enrollment)
        .bind(device)
        .bind(hash(&code))
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(expires) = expires {
            return Ok(
                json!({"id":enrollment,"device_id":device,"code":code,"expires_at":expires}),
            );
        }
    }
    Err(Error::Unavailable)
}

pub(super) async fn prepare(pool: &PgPool, command: &mut Command) -> Result<()> {
    let Command::Enroll {
        device_id,
        code,
        client_instance_id,
        ..
    } = command
    else {
        return Err(invalid());
    };
    let normalized: String = code.chars().filter(|c| *c != ' ' && *c != '-').collect();
    // One database-backed budget across API versions and replicas. Rotating codes or
    // caller-controlled instance IDs cannot evade the global budget.
    let mut attempts = pool.begin().await?;
    sqlx::query("DELETE FROM device_pairing_attempt_windows WHERE reset_at < clock_timestamp()-interval '1 day'")
        .execute(&mut *attempts).await?;
    for (scope, limit) in [
        ("global".to_owned(), 100),
        (format!("instance:{}", hash(client_instance_id)), 5),
        (format!("code:{}", hash(&normalized)), 5),
    ] {
        let count: i32 = sqlx::query_scalar(
            "INSERT INTO device_pairing_attempt_windows(scope,requests,reset_at)
             VALUES($1,1,clock_timestamp()+interval '10 minutes')
             ON CONFLICT(scope) DO UPDATE SET
             requests=CASE WHEN device_pairing_attempt_windows.reset_at<=clock_timestamp()
               THEN 1 ELSE LEAST(device_pairing_attempt_windows.requests+1,101) END,
             reset_at=CASE WHEN device_pairing_attempt_windows.reset_at<=clock_timestamp()
               THEN clock_timestamp()+interval '10 minutes' ELSE device_pairing_attempt_windows.reset_at END
             RETURNING requests",
        ).bind(scope).fetch_one(&mut *attempts).await?;
        if count > limit {
            attempts.commit().await?;
            return Err(Error::Policy("pairing_rate_limited"));
        }
    }
    attempts.commit().await?;
    let short = normalized.len() == 8 && normalized.bytes().all(|b| b.is_ascii_digit());
    let legacy = device_id.is_some()
        && normalized.len() == 64
        && normalized.bytes().all(|b| b.is_ascii_hexdigit());
    if code.len() > 128 || (!short && !legacy) || text(client_instance_id, 1, 200).is_err() {
        return Err(invalid());
    }
    *code = normalized;
    if device_id.is_none() {
        // Include consumed codes for idempotent response lookup; apply() still
        // atomically checks expiry and one-time consumption before issuing a token.
        *device_id =
            sqlx::query_scalar("SELECT device_id FROM device_enrollments WHERE code_hash=$1")
                .bind(hash(code))
                .fetch_optional(pool)
                .await?;
    }
    if device_id.is_none() {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codes_are_eight_digits_including_leading_zeroes() {
        for _ in 0..1024 {
            let code = random_code();
            assert_eq!(code.len(), 8);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }
}
