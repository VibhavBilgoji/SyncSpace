//! Input rules for registration and login (`docs/API.md` section 2, `docs/DATA_MODEL.md`).
//!
//! Every rule that fails is reported at once, as `details` of a 422, so a form can show all
//! problems in one round trip. Lengths count characters (Unicode scalar values), not bytes: a
//! password of 8 emoji is 8 characters. Emails are ASCII only and stored lower-cased.

use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;

pub const MAX_NAME_CHARS: usize = 80;
pub const MIN_PASSWORD_CHARS: usize = 8;
pub const MAX_PASSWORD_CHARS: usize = 128;
pub const MAX_EMAIL_CHARS: usize = 254;
const MAX_LOCAL_PART_CHARS: usize = 64;
const MAX_DOMAIN_LABEL_CHARS: usize = 63;

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "RegisterRequest")]
pub struct RegisterInput {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "LoginRequest")]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

/// A registration that passed every rule, with the name trimmed and the email normalised.
#[derive(Debug, PartialEq, Eq)]
pub struct Registration {
    pub name: String,
    pub email: String,
    pub password: String,
}

pub fn validate_registration(input: RegisterInput) -> Result<Registration, ApiError> {
    let mut problems: Vec<(&str, &str)> = Vec::new();

    let name = input.name.trim().to_string();
    let name_length = name.chars().count();
    if name_length == 0 || name_length > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        problems.push((
            "name",
            "must be 1 to 80 characters and contain no control characters",
        ));
    }

    let email = normalise_email(&input.email);
    if email.is_none() {
        problems.push(("email", "must be a valid email address"));
    }

    let password_length = input.password.chars().count();
    if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&password_length) {
        problems.push(("password", "must be 8 to 128 characters"));
    }

    match email {
        Some(email) if problems.is_empty() => Ok(Registration {
            name,
            email,
            password: input.password,
        }),
        _ => Err(ApiError::validation(problems)),
    }
}

/// Trimmed, lower-cased email, or `None` when it is not an acceptable address.
///
/// The rules are deliberately plain: ASCII only, at most 254 characters, one `@`, a local part
/// of 1 to 64 allowed characters without leading, trailing or doubled dots, and a domain of at
/// least two dot-separated labels of letters, digits and inner hyphens.
pub fn normalise_email(raw: &str) -> Option<String> {
    let email = raw.trim();
    if email.is_empty() || email.len() > MAX_EMAIL_CHARS || !email.is_ascii() {
        return None;
    }
    let email = email.to_ascii_lowercase();

    let (local, domain) = email.split_once('@')?;
    if domain.contains('@') || !is_valid_local_part(local) || !is_valid_domain(domain) {
        return None;
    }
    Some(email)
}

fn is_valid_local_part(local: &str) -> bool {
    const ALLOWED_SYMBOLS: &str = ".!#$%&'*+/=?^_`{|}~-";
    !local.is_empty()
        && local.len() <= MAX_LOCAL_PART_CHARS
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ALLOWED_SYMBOLS.contains(c))
}

fn is_valid_domain(domain: &str) -> bool {
    let labels: Vec<&str> = domain.split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= MAX_DOMAIN_LABEL_CHARS
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(name: &str, email: &str, password: &str) -> RegisterInput {
        RegisterInput {
            name: name.into(),
            email: email.into(),
            password: password.into(),
        }
    }

    fn fields_with_problems(input: RegisterInput) -> Vec<String> {
        match validate_registration(input) {
            Err(ApiError::Validation(fields)) => fields.into_keys().collect(),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn a_good_registration_is_normalised() {
        let registration = validate_registration(input(
            "  Ada Lovelace  ",
            "  Ada@Example.COM ",
            "a long enough passphrase",
        ))
        .unwrap();
        assert_eq!(
            registration,
            Registration {
                name: "Ada Lovelace".into(),
                email: "ada@example.com".into(),
                password: "a long enough passphrase".into(),
            }
        );
    }

    #[test]
    fn the_password_is_never_trimmed() {
        let registration =
            validate_registration(input("Ada", "ada@example.com", "  spaces  ")).unwrap();
        assert_eq!(registration.password, "  spaces  ");
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        assert_eq!(
            fields_with_problems(input("", "not-an-email", "short")),
            vec!["email", "name", "password"]
        );
    }

    #[test]
    fn name_length_is_counted_in_characters_at_both_edges() {
        assert!(validate_registration(input("A", "a@b.co", "12345678")).is_ok());
        assert!(validate_registration(input(&"n".repeat(80), "a@b.co", "12345678")).is_ok());
        assert_eq!(
            fields_with_problems(input(&"n".repeat(81), "a@b.co", "12345678")),
            vec!["name"]
        );
        // 80 emoji are 80 characters (320 bytes) and must pass; 81 must not.
        assert!(validate_registration(input(&"🙂".repeat(80), "a@b.co", "12345678")).is_ok());
        assert_eq!(
            fields_with_problems(input(&"🙂".repeat(81), "a@b.co", "12345678")),
            vec!["name"]
        );
    }

    #[test]
    fn a_blank_name_or_one_with_control_characters_is_refused() {
        for name in ["", "   ", "Ada\nLovelace", "Ada\u{0}", "\tAda\u{7}"] {
            assert_eq!(
                fields_with_problems(input(name, "a@b.co", "12345678")),
                vec!["name"],
                "{name:?}"
            );
        }
    }

    #[test]
    fn password_length_is_counted_in_characters_at_both_edges() {
        assert_eq!(
            fields_with_problems(input("Ada", "a@b.co", "1234567")),
            vec!["password"]
        );
        assert!(validate_registration(input("Ada", "a@b.co", "12345678")).is_ok());
        assert!(validate_registration(input("Ada", "a@b.co", &"p".repeat(128))).is_ok());
        assert_eq!(
            fields_with_problems(input("Ada", "a@b.co", &"p".repeat(129))),
            vec!["password"]
        );
        // 8 emoji are 8 characters although they are 32 bytes; 7 are not enough.
        assert!(validate_registration(input("Ada", "a@b.co", &"🙂".repeat(8))).is_ok());
        assert_eq!(
            fields_with_problems(input("Ada", "a@b.co", &"🙂".repeat(7))),
            vec!["password"]
        );
    }

    #[test]
    fn acceptable_emails_are_normalised() {
        for (raw, expected) in [
            ("ada@example.com", "ada@example.com"),
            ("  ADA@EXAMPLE.COM  ", "ada@example.com"),
            (
                "first.last+tag@sub.example.co.in",
                "first.last+tag@sub.example.co.in",
            ),
            ("o'brien@example.org", "o'brien@example.org"),
            ("a@b.co", "a@b.co"),
            ("user_name@my-domain.com", "user_name@my-domain.com"),
            ("1@2.io", "1@2.io"),
        ] {
            assert_eq!(normalise_email(raw).as_deref(), Some(expected), "{raw}");
        }
    }

    #[test]
    fn unacceptable_emails_are_refused() {
        let too_long_local = format!("{}@example.com", "a".repeat(65));
        let too_long_label = format!("a@{}.com", "d".repeat(64));
        let too_long_total = format!("a@{}.com", format!("{}.", "d".repeat(60)).repeat(4));
        for raw in [
            "",
            "   ",
            "plain",
            "@example.com",
            "ada@",
            "ada@example",
            "ada@@example.com",
            "ada@exa@mple.com",
            "ada example@example.com",
            "ada@exa mple.com",
            ".ada@example.com",
            "ada.@example.com",
            "a..da@example.com",
            "ada@-example.com",
            "ada@example-.com",
            "ada@.example.com",
            "ada@example..com",
            "ada@example.com.",
            "ada@exam_ple.com",
            "adá@example.com",
            "ada@exámple.com",
            "ada<script>@example.com",
            "ada\n@example.com",
            &too_long_local,
            &too_long_label,
            &too_long_total,
        ] {
            assert_eq!(normalise_email(raw), None, "{raw:?} must be refused");
        }
    }

    #[test]
    fn the_email_length_limit_is_254_characters() {
        // 64 + 1 + 189 = 254 characters in total.
        let domain_tail = format!(
            "{}.{}.{}.com",
            "d".repeat(60),
            "d".repeat(60),
            "d".repeat(60)
        );
        let ok = format!("{}@{domain_tail}", "a".repeat(64));
        assert!(ok.len() <= 254);
        assert!(normalise_email(&ok).is_some());
        let too_long = format!("{}@{}.{domain_tail}", "a".repeat(64), "d".repeat(60));
        assert!(too_long.len() > 254);
        assert_eq!(normalise_email(&too_long), None);
    }
}
