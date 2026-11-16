//! Input rules for workspaces and members (`docs/DATA_MODEL.md` section 2, `docs/API.md`).
//!
//! As in `auth::validation`, every broken rule is reported at once and lengths count characters.
//! A slug is never changed for the caller: it either matches
//! `^[a-z0-9](?:[a-z0-9-]{1,38}[a-z0-9])$` or it is rejected.

use serde::Deserialize;
use ts_rs::TS;

use crate::auth::validation::normalise_email;
use crate::error::ApiError;
use crate::rbac::Role;

pub const MAX_NAME_CHARS: usize = 60;
pub const MIN_SLUG_CHARS: usize = 3;
pub const MAX_SLUG_CHARS: usize = 40;

const NAME_PROBLEM: &str = "must be 1 to 60 characters and contain no control characters";
const SLUG_PROBLEM: &str = "must be 3 to 40 characters: lower-case letters, digits and hyphens, starting and ending with a letter or digit";

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "CreateWorkspaceRequest")]
pub struct CreateInput {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "UpdateWorkspaceRequest")]
pub struct PatchInput {
    #[ts(optional)]
    pub name: Option<String>,
    #[ts(optional)]
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "AddMemberRequest")]
pub struct AddMemberInput {
    pub email: String,
    pub role: Role,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "ChangeRoleRequest")]
pub struct RoleInput {
    pub role: Role,
}

/// The trimmed name, or `None` when it breaks the rules.
pub fn clean_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    let length = name.chars().count();
    if length == 0 || length > MAX_NAME_CHARS || name.chars().any(char::is_control) {
        None
    } else {
        Some(name.to_string())
    }
}

pub fn is_valid_slug(slug: &str) -> bool {
    let bytes = slug.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    (MIN_SLUG_CHARS..=MAX_SLUG_CHARS).contains(&bytes.len())
        && edge(bytes[0])
        && edge(bytes[bytes.len() - 1])
        && bytes.iter().all(|&b| edge(b) || b == b'-')
}

pub fn validate_create(input: CreateInput) -> Result<(String, String), ApiError> {
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let name = clean_name(&input.name);
    if name.is_none() {
        problems.push(("name", NAME_PROBLEM));
    }
    if !is_valid_slug(&input.slug) {
        problems.push(("slug", SLUG_PROBLEM));
    }
    match name {
        Some(name) if problems.is_empty() => Ok((name, input.slug)),
        _ => Err(ApiError::validation(problems)),
    }
}

/// At least one of `name` and `slug` must be present, and each present one must be valid.
pub fn validate_patch(input: PatchInput) -> Result<(Option<String>, Option<String>), ApiError> {
    if input.name.is_none() && input.slug.is_none() {
        return Err(ApiError::validation([(
            "body",
            "send at least one of name and slug",
        )]));
    }
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let name = input.name.as_deref().map(clean_name);
    if matches!(name, Some(None)) {
        problems.push(("name", NAME_PROBLEM));
    }
    if input
        .slug
        .as_deref()
        .is_some_and(|slug| !is_valid_slug(slug))
    {
        problems.push(("slug", SLUG_PROBLEM));
    }
    if problems.is_empty() {
        Ok((name.flatten(), input.slug))
    } else {
        Err(ApiError::validation(problems))
    }
}

/// The normalised email of a member to add, or a 422.
pub fn validate_member_email(raw: &str) -> Result<String, ApiError> {
    normalise_email(raw)
        .ok_or_else(|| ApiError::validation([("email", "must be a valid email address")]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(error: ApiError) -> Vec<String> {
        match error {
            ApiError::Validation(map) => map.into_keys().collect(),
            other => panic!("expected a validation error, got {other:?}"),
        }
    }

    #[test]
    fn slug_edges() {
        let longest = "a".repeat(40);
        let hyphens = format!("a{}b", "-".repeat(38));
        let too_long = "a".repeat(41);
        let ok = [
            "abc",
            "a-b",
            "a1b",
            "team-42",
            "0day",
            longest.as_str(),
            hyphens.as_str(),
        ];
        for slug in ok {
            assert!(is_valid_slug(slug), "{slug}");
        }
        let bad = [
            "",
            "a",
            "ab",
            "-ab",
            "ab-",
            "-abc-",
            "Abc",
            "aBc",
            "a_b",
            "a b",
            "a.b",
            "ünï",
            too_long.as_str(),
            " abc",
            "abc\n",
        ];
        for slug in bad {
            assert!(!is_valid_slug(slug), "{slug:?}");
        }
    }

    #[test]
    fn name_edges_count_characters_and_trim() {
        assert_eq!(
            clean_name("  Engineering Team  ").unwrap(),
            "Engineering Team"
        );
        assert!(clean_name(&"é".repeat(60)).is_some());
        assert!(clean_name(&"é".repeat(61)).is_none());
        assert!(clean_name("").is_none());
        assert!(clean_name("   ").is_none());
        assert!(clean_name("tab\there").is_none());
        assert!(clean_name("line\nbreak").is_none());
    }

    #[test]
    fn create_reports_every_problem_at_once() {
        let error = validate_create(CreateInput {
            name: " ".into(),
            slug: "Bad Slug".into(),
        })
        .unwrap_err();
        assert_eq!(fields(error), ["name", "slug"]);

        let (name, slug) = validate_create(CreateInput {
            name: " Team ".into(),
            slug: "team".into(),
        })
        .unwrap();
        assert_eq!((name.as_str(), slug.as_str()), ("Team", "team"));
    }

    #[test]
    fn patch_needs_a_field_and_checks_the_ones_present() {
        let nothing = validate_patch(PatchInput {
            name: None,
            slug: None,
        })
        .unwrap_err();
        assert_eq!(fields(nothing), ["body"]);

        let only_name = validate_patch(PatchInput {
            name: Some(" New ".into()),
            slug: None,
        })
        .unwrap();
        assert_eq!(only_name, (Some("New".to_string()), None));

        let only_slug = validate_patch(PatchInput {
            name: None,
            slug: Some("new-slug".into()),
        })
        .unwrap();
        assert_eq!(only_slug, (None, Some("new-slug".to_string())));

        let both_bad = validate_patch(PatchInput {
            name: Some(String::new()),
            slug: Some("X".into()),
        })
        .unwrap_err();
        assert_eq!(fields(both_bad), ["name", "slug"]);
    }

    #[test]
    fn member_email_is_normalised_like_at_login() {
        assert_eq!(
            validate_member_email("  Ada@Example.COM ").unwrap(),
            "ada@example.com"
        );
        assert_eq!(
            fields(validate_member_email("nope").unwrap_err()),
            ["email"]
        );
    }
}
