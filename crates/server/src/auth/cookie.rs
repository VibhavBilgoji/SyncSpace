//! The session cookie `ss_session` (`docs/API.md` section 1): built and parsed by hand, since a
//! fixed cookie name needs no cookie library.
//!
//! `HttpOnly` keeps scripts away from the token, `SameSite=Lax` stops cross-site POSTs from
//! carrying it, and `Secure` (when `COOKIE_SECURE` is true, always in production) keeps it off
//! plain HTTP.

use super::session::SESSION_SECONDS;

pub const COOKIE_NAME: &str = "ss_session";

/// A `Set-Cookie` value that starts a session.
pub fn session_cookie(token: &str, secure: bool) -> String {
    format!(
        "{COOKIE_NAME}={token}; Max-Age={SESSION_SECONDS}; Path=/; HttpOnly; SameSite=Lax{}",
        secure_flag(secure)
    )
}

/// A `Set-Cookie` value that ends the session: the same cookie, empty and already expired.
pub fn clearing_cookie(secure: bool) -> String {
    format!(
        "{COOKIE_NAME}=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/; HttpOnly; SameSite=Lax{}",
        secure_flag(secure)
    )
}

fn secure_flag(secure: bool) -> &'static str {
    if secure { "; Secure" } else { "" }
}

/// The value of the first `ss_session` cookie among the given `Cookie` header values. Pairs
/// that are malformed, have another name or have an empty value are skipped.
pub fn find_session_token<'a>(headers: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    headers
        .into_iter()
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .filter(|(name, _)| name.trim() == COOKIE_NAME)
        .map(|(_, value)| value.trim().trim_matches('"'))
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_has_the_documented_attributes() {
        let cookie = session_cookie("token.value.here", false);
        assert_eq!(
            cookie,
            "ss_session=token.value.here; Max-Age=86400; Path=/; HttpOnly; SameSite=Lax"
        );
    }

    #[test]
    fn secure_follows_the_setting_and_nothing_else_changes() {
        let plain = session_cookie("t", false);
        let secure = session_cookie("t", true);
        assert!(!plain.contains("Secure"));
        assert_eq!(secure, format!("{plain}; Secure"));
        let plain = clearing_cookie(false);
        let secure = clearing_cookie(true);
        assert!(!plain.contains("Secure"));
        assert_eq!(secure, format!("{plain}; Secure"));
    }

    #[test]
    fn clearing_the_cookie_expires_it_at_once_for_the_same_path() {
        let cookie = clearing_cookie(true);
        assert!(
            cookie.starts_with("ss_session=;"),
            "the value is empty: {cookie}"
        );
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("Expires=Thu, 01 Jan 1970 00:00:00 GMT"));
        // Path and flags must match the cookie being replaced, or the browser keeps both.
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
    }

    #[test]
    fn the_token_is_found_among_other_cookies() {
        let header = "theme=dark; ss_session=abc.def.ghi; other=1";
        assert_eq!(find_session_token([header]), Some("abc.def.ghi"));
    }

    #[test]
    fn other_cookie_names_are_ignored_even_when_they_look_similar() {
        for header in [
            "theme=dark",
            "ss_session_old=abc",
            "xss_session=abc",
            "SS_SESSION=abc",
            "",
        ] {
            assert_eq!(find_session_token([header]), None, "{header:?}");
        }
    }

    #[test]
    fn the_first_session_cookie_wins() {
        assert_eq!(
            find_session_token(["ss_session=first; ss_session=second"]),
            Some("first")
        );
        assert_eq!(
            find_session_token(["a=1", "ss_session=from-the-second-header"]),
            Some("from-the-second-header")
        );
    }

    #[test]
    fn an_empty_value_is_skipped_and_a_later_one_is_used() {
        assert_eq!(
            find_session_token(["ss_session=; ss_session=real"]),
            Some("real")
        );
        assert_eq!(find_session_token(["ss_session="]), None);
    }

    #[test]
    fn whitespace_and_quotes_are_tolerated() {
        assert_eq!(find_session_token(["  ss_session = tok  "]), Some("tok"));
        assert_eq!(
            find_session_token(["ss_session=\"quoted\""]),
            Some("quoted")
        );
    }

    #[test]
    fn malformed_headers_give_no_token_and_never_panic() {
        for header in [
            ";;;",
            "=",
            "ss_session",
            "=ss_session",
            "ss_session;value",
            "\u{0}",
        ] {
            assert_eq!(find_session_token([header]), None, "{header:?}");
        }
    }
}
