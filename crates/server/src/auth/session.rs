//! The session token: an HS256 JWT in the `ss_session` cookie (`docs/API.md` section 1).
//!
//! Claims: `sub` (the user id as 24 hex characters), `iat` and `exp` (24 hours after `iat`).
//! Verification accepts only HS256, requires `exp` and `sub`, and allows no clock leeway (the
//! library's default is 60 seconds). Tokens are issued by a function that takes the current
//! time as an argument, so expiry can be tested without waiting or mocking a clock.
//!
//! A token cannot be revoked before it expires: it is stateless by design, and logout only
//! clears the cookie.

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::errors::ErrorKind;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use mongodb::bson::oid::ObjectId;
use serde::{Deserialize, Serialize};

/// How long a session lasts (`docs/PRD.md` FR1).
pub const SESSION_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iat: u64,
    exp: u64,
}

/// Why a token was refused. Callers answer 401 for every variant and never say which.
#[derive(Debug, PartialEq, Eq)]
pub enum SessionError {
    Expired,
    Invalid,
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

/// A token for `user`, issued at `issued_at` (Unix seconds).
pub fn issue(
    secret: &[u8],
    user: ObjectId,
    issued_at: u64,
) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims {
        sub: user.to_hex(),
        iat: issued_at,
        exp: issued_at + SESSION_SECONDS,
    };
    encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret)?,
    )
}

/// The user a token belongs to, if the token is genuine and has not expired.
pub fn verify(secret: &[u8], token: &str) -> Result<ObjectId, SessionError> {
    let key = DecodingKey::from_secret(secret).map_err(|_| SessionError::Invalid)?;

    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.required_spec_claims = ["exp", "sub"].into_iter().map(String::from).collect();

    let data = decode::<Claims>(token, &key, &validation).map_err(|error| {
        if matches!(error.kind(), ErrorKind::ExpiredSignature) {
            SessionError::Expired
        } else {
            SessionError::Invalid
        }
    })?;
    ObjectId::parse_str(&data.claims.sub).map_err(|_| SessionError::Invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    const SECRET: &[u8] = &[7u8; 32];
    const OTHER_SECRET: &[u8] = &[9u8; 32];

    fn user() -> ObjectId {
        ObjectId::parse_str("65a1b2c3d4e5f60718293a4b").unwrap()
    }

    fn claims_of(token: &str) -> serde_json::Value {
        let payload = token.split('.').nth(1).unwrap();
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap()
    }

    fn encode_raw(header: &serde_json::Value, claims: &serde_json::Value, secret: &[u8]) -> String {
        // Build and sign a token by hand, to craft ones the issuer would never produce.
        use hmac_signing::sign;
        let header = URL_SAFE_NO_PAD.encode(header.to_string());
        let claims = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signature = sign(secret, &format!("{header}.{claims}"));
        format!("{header}.{claims}.{signature}")
    }

    /// Tiny HS256 signer for the tests, built on jsonwebtoken itself.
    mod hmac_signing {
        use jsonwebtoken::{Algorithm, EncodingKey};

        pub fn sign(secret: &[u8], message: &str) -> String {
            jsonwebtoken::crypto::sign(
                message.as_bytes(),
                &EncodingKey::from_secret(secret).unwrap(),
                Algorithm::HS256,
            )
            .unwrap()
        }
    }

    #[test]
    fn a_fresh_token_returns_its_user() {
        let token = issue(SECRET, user(), now()).unwrap();
        assert_eq!(verify(SECRET, &token), Ok(user()));
    }

    #[test]
    fn a_session_lasts_exactly_24_hours() {
        let token = issue(SECRET, user(), 1_700_000_000).unwrap();
        let claims = claims_of(&token);
        assert_eq!(claims["iat"], 1_700_000_000u64);
        assert_eq!(claims["exp"], 1_700_000_000u64 + 86_400);
        assert_eq!(claims["sub"], "65a1b2c3d4e5f60718293a4b");
    }

    #[test]
    fn an_expired_token_is_refused_with_no_leeway() {
        // Issued 24 hours and 5 seconds ago: expired by 5 seconds, well inside the library's
        // default 60 second leeway, so this only passes because the leeway is 0.
        let token = issue(SECRET, user(), now() - SESSION_SECONDS - 5).unwrap();
        assert_eq!(verify(SECRET, &token), Err(SessionError::Expired));
    }

    #[test]
    fn a_token_that_is_still_valid_just_before_expiry_is_accepted() {
        let token = issue(SECRET, user(), now() - SESSION_SECONDS + 30).unwrap();
        assert_eq!(verify(SECRET, &token), Ok(user()));
    }

    #[test]
    fn a_token_signed_with_another_key_is_refused() {
        let token = issue(OTHER_SECRET, user(), now()).unwrap();
        assert_eq!(verify(SECRET, &token), Err(SessionError::Invalid));
    }

    #[test]
    fn a_tampered_payload_is_refused() {
        let token = issue(SECRET, user(), now()).unwrap();
        let mut parts: Vec<String> = token.split('.').map(String::from).collect();
        let mut claims = claims_of(&token);
        claims["sub"] = serde_json::json!("000000000000000000000001");
        parts[1] = URL_SAFE_NO_PAD.encode(claims.to_string());
        assert_eq!(verify(SECRET, &parts.join(".")), Err(SessionError::Invalid));
    }

    #[test]
    fn the_none_algorithm_is_refused() {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
        let claims = URL_SAFE_NO_PAD.encode(
            serde_json::json!({"sub": user().to_hex(), "iat": now(), "exp": now() + 3600})
                .to_string(),
        );
        for token in [format!("{header}.{claims}."), format!("{header}.{claims}")] {
            assert_eq!(
                verify(SECRET, &token),
                Err(SessionError::Invalid),
                "{token}"
            );
        }
    }

    #[test]
    fn another_hmac_algorithm_with_the_same_secret_is_refused() {
        let claims = serde_json::json!({"sub": user().to_hex(), "iat": now(), "exp": now() + 3600});
        let token = jsonwebtoken::encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(SECRET).unwrap(),
        )
        .unwrap();
        assert_eq!(verify(SECRET, &token), Err(SessionError::Invalid));
    }

    #[test]
    fn a_token_without_exp_or_sub_is_refused() {
        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let without_exp = encode_raw(
            &header,
            &serde_json::json!({"sub": user().to_hex()}),
            SECRET,
        );
        let without_sub = encode_raw(&header, &serde_json::json!({"exp": now() + 3600}), SECRET);
        assert_eq!(verify(SECRET, &without_exp), Err(SessionError::Invalid));
        assert_eq!(verify(SECRET, &without_sub), Err(SessionError::Invalid));
    }

    #[test]
    fn a_hand_built_token_with_the_right_shape_is_accepted_so_the_refusals_above_mean_something() {
        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let claims = serde_json::json!({"sub": user().to_hex(), "iat": now(), "exp": now() + 3600});
        assert_eq!(
            verify(SECRET, &encode_raw(&header, &claims, SECRET)),
            Ok(user())
        );
    }

    #[test]
    fn a_subject_that_is_not_an_object_id_is_refused() {
        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let claims = serde_json::json!({"sub": "not-an-id", "iat": now(), "exp": now() + 3600});
        assert_eq!(
            verify(SECRET, &encode_raw(&header, &claims, SECRET)),
            Err(SessionError::Invalid)
        );
    }

    #[test]
    fn garbage_is_refused_and_never_panics() {
        for garbage in [
            "",
            ".",
            "..",
            "a.b.c",
            "not a token",
            "eyJhbGciOiJIUzI1NiJ9",
            "\0\0",
        ] {
            assert_eq!(
                verify(SECRET, garbage),
                Err(SessionError::Invalid),
                "{garbage:?}"
            );
        }
    }
}
