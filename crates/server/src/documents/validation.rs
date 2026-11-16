//! Input rules for documents (`docs/DATA_MODEL.md` section 2, `docs/API.md`).
//!
//! As elsewhere, every broken rule is reported at once and lengths count characters.

use serde::Deserialize;
use ts_rs::TS;

use crate::error::ApiError;

pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_ICON_CHARS: usize = 16;
/// The icon of a document created without one.
pub const DEFAULT_ICON: &str = "📄";

const TITLE_PROBLEM: &str = "must be 1 to 120 characters and contain no control characters";
const ICON_PROBLEM: &str =
    "must be 1 to 16 characters (one emoji or a short icon key) and contain no control characters";

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "CreateDocumentRequest")]
pub struct CreateInput {
    pub title: String,
    #[ts(optional)]
    pub icon: Option<String>,
}

#[derive(Debug, Deserialize, TS)]
#[ts(rename = "UpdateDocumentRequest")]
pub struct PatchInput {
    #[ts(optional)]
    pub title: Option<String>,
    #[ts(optional)]
    pub icon: Option<String>,
}

fn clean(raw: &str, max_chars: usize) -> Option<String> {
    let text = raw.trim();
    let length = text.chars().count();
    if length == 0 || length > max_chars || text.chars().any(char::is_control) {
        None
    } else {
        Some(text.to_string())
    }
}

pub fn clean_title(raw: &str) -> Option<String> {
    clean(raw, MAX_TITLE_CHARS)
}

pub fn clean_icon(raw: &str) -> Option<String> {
    clean(raw, MAX_ICON_CHARS)
}

/// The title and icon of a new document (the icon defaults).
pub fn validate_create(input: CreateInput) -> Result<(String, String), ApiError> {
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let title = clean_title(&input.title);
    if title.is_none() {
        problems.push(("title", TITLE_PROBLEM));
    }
    let icon = match input.icon.as_deref() {
        None => Some(DEFAULT_ICON.to_string()),
        Some(raw) => clean_icon(raw),
    };
    if icon.is_none() {
        problems.push(("icon", ICON_PROBLEM));
    }
    match (title, icon) {
        (Some(title), Some(icon)) if problems.is_empty() => Ok((title, icon)),
        _ => Err(ApiError::validation(problems)),
    }
}

/// At least one of `title` and `icon` must be present, and each present one must be valid.
pub fn validate_patch(input: PatchInput) -> Result<(Option<String>, Option<String>), ApiError> {
    if input.title.is_none() && input.icon.is_none() {
        return Err(ApiError::validation([(
            "body",
            "send at least one of title and icon",
        )]));
    }
    let mut problems: Vec<(&str, &str)> = Vec::new();
    let title = input.title.as_deref().map(clean_title);
    if matches!(title, Some(None)) {
        problems.push(("title", TITLE_PROBLEM));
    }
    let icon = input.icon.as_deref().map(clean_icon);
    if matches!(icon, Some(None)) {
        problems.push(("icon", ICON_PROBLEM));
    }
    if problems.is_empty() {
        Ok((title.flatten(), icon.flatten()))
    } else {
        Err(ApiError::validation(problems))
    }
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
    fn titles_count_characters_and_trim() {
        assert_eq!(clean_title("  Plan  ").unwrap(), "Plan");
        assert!(clean_title(&"é".repeat(120)).is_some());
        assert!(clean_title(&"é".repeat(121)).is_none());
        for bad in ["", "   ", "tab\there", "line\nbreak"] {
            assert!(clean_title(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn icons_are_short_and_may_be_an_emoji() {
        assert_eq!(clean_icon("🚀").unwrap(), "🚀");
        assert!(clean_icon("rocket").is_some());
        assert!(clean_icon(&"x".repeat(16)).is_some());
        assert!(clean_icon(&"x".repeat(17)).is_none());
        assert!(clean_icon("").is_none());
        assert!(clean_icon("a\u{0007}").is_none());
    }

    #[test]
    fn create_defaults_the_icon_and_reports_every_problem() {
        let (title, icon) = validate_create(CreateInput {
            title: " Notes ".into(),
            icon: None,
        })
        .unwrap();
        assert_eq!((title.as_str(), icon.as_str()), ("Notes", DEFAULT_ICON));

        let error = validate_create(CreateInput {
            title: String::new(),
            icon: Some("x".repeat(40)),
        })
        .unwrap_err();
        assert_eq!(fields(error), ["icon", "title"]);
    }

    #[test]
    fn patch_needs_a_field_and_checks_the_ones_present() {
        let nothing = validate_patch(PatchInput {
            title: None,
            icon: None,
        })
        .unwrap_err();
        assert_eq!(fields(nothing), ["body"]);

        let only_icon = validate_patch(PatchInput {
            title: None,
            icon: Some("🔥".into()),
        })
        .unwrap();
        assert_eq!(only_icon, (None, Some("🔥".to_string())));

        let both_bad = validate_patch(PatchInput {
            title: Some(" ".into()),
            icon: Some(String::new()),
        })
        .unwrap_err();
        assert_eq!(fields(both_bad), ["icon", "title"]);
    }
}
