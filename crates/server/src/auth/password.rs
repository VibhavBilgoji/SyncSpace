//! Password hashing with argon2id at the OWASP baseline (`docs/ARCHITECTURE.md` section 5):
//! 19 MiB of memory, 2 iterations, 1 lane. The result is a PHC string such as
//! `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`, which carries its own parameters: verifying
//! reads them from the stored string, so raising the cost later does not lock anyone out.
//!
//! Both functions are slow on purpose (tens of milliseconds, 19 MiB). Callers run them on a
//! blocking thread and behind a limit; see [`crate::auth::AuthService`].

use std::fmt;

use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};

/// Memory cost in KiB: 19 MiB.
pub const MEMORY_KIB: u32 = 19 * 1024;
pub const ITERATIONS: u32 = 2;
pub const LANES: u32 = 1;

/// Hashing failed. The message is for the log only.
#[derive(Debug)]
pub struct HashError(String);

impl fmt::Display for HashError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "password hashing failed: {}", self.0)
    }
}

impl std::error::Error for HashError {}

fn argon2() -> Result<Argon2<'static>, HashError> {
    let params = Params::new(MEMORY_KIB, ITERATIONS, LANES, None)
        .map_err(|error| HashError(error.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

/// Hashes a password with a fresh random salt.
pub fn hash(password: &str) -> Result<String, HashError> {
    let hash = argon2()?
        .hash_password(password.as_bytes())
        .map_err(|error| HashError(error.to_string()))?;
    Ok(hash.to_string())
}

/// Whether `password` matches the stored PHC string. A stored value that is not a valid hash
/// counts as a mismatch: it must never panic or let anyone in.
pub fn verify(password: &str, stored: &str) -> bool {
    match argon2() {
        Ok(argon2) => argon2.verify_password(password.as_bytes(), stored).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_an_argon2id_phc_string_with_the_documented_cost() {
        let stored = hash("correct horse battery staple").unwrap();
        assert!(
            stored.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "unexpected format: {stored}"
        );
    }

    #[test]
    fn the_hash_does_not_contain_the_password() {
        let password = "a-very-recognisable-password";
        let stored = hash(password).unwrap();
        assert!(!stored.contains(password));
        assert!(!stored.contains("recognisable"));
    }

    #[test]
    fn the_same_password_hashes_differently_every_time() {
        let first = hash("same password").unwrap();
        let second = hash("same password").unwrap();
        assert_ne!(first, second, "the salt must be random");
        assert!(verify("same password", &first));
        assert!(verify("same password", &second));
    }

    #[test]
    fn verify_accepts_the_right_password_and_rejects_others() {
        let stored = hash("hunter2hunter2").unwrap();
        assert!(verify("hunter2hunter2", &stored));
        assert!(!verify("hunter2hunter3", &stored));
        assert!(!verify("", &stored));
        assert!(!verify("HUNTER2HUNTER2", &stored));
    }

    #[test]
    fn unicode_passwords_round_trip() {
        let password = "pässwörd-🙂-नमस्ते";
        let stored = hash(password).unwrap();
        assert!(verify(password, &stored));
        assert!(!verify("password-🙂-नमस्ते", &stored));
    }

    #[test]
    fn a_stored_value_that_is_not_a_hash_is_a_mismatch_and_never_a_panic() {
        for garbage in [
            "",
            "plaintext-password",
            "$argon2id$",
            "$argon2id$v=19$m=19456,t=2,p=1$",
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHQ$",
            "$2b$12$abcdefghijklmnopqrstuuABCDEFGHIJKLMNOPQRSTUVWXYZ01234",
            "\0\0\0",
        ] {
            assert!(!verify("anything", garbage), "{garbage:?} must not verify");
        }
    }

    #[test]
    fn a_hash_made_with_other_parameters_still_verifies_from_its_own_string() {
        // 8 MiB, 1 iteration: cheaper than ours, as an older deployment might have used.
        let weaker = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(8 * 1024, 1, 1, None).unwrap(),
        );
        let stored = weaker.hash_password(b"old password").unwrap().to_string();
        assert!(stored.contains("m=8192,t=1,p=1"));
        assert!(verify("old password", &stored));
        assert!(!verify("new password", &stored));
    }
}
