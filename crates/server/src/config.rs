//! Server configuration, read from environment variables (`docs/DEPLOYMENT.md` section 5).
//!
//! The server fails fast: every problem is collected and reported together, so one start-up
//! attempt shows everything that is wrong. Secrets (keys, the MongoDB URI with its password)
//! are wrapped in [`Secret`], whose `Debug` output never shows the value, so a stray
//! `{:?}` or a startup log line cannot leak them.

use std::collections::HashMap;
use std::fmt;

use axum::http::Uri;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Smallest accepted signing key, as decoded bytes (`docs/ARCHITECTURE.md` section 5).
const MIN_SECRET_BYTES: usize = 32;

/// A value that must never be printed. `Debug` shows a placeholder.
#[derive(Clone)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }

    /// The real value. Call this only where it is actually needed.
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    /// TCP port to bind on `0.0.0.0`. Render sets `PORT`.
    pub port: u16,
    /// Contains credentials for Atlas, hence a secret.
    pub mongodb_uri: Secret<String>,
    pub mongodb_db: String,
    /// Decoded signing key for the session JWT (HS256).
    pub jwt_secret: Secret<Vec<u8>>,
    /// Decoded key for WebSocket tickets. Always different from `jwt_secret`.
    pub ticket_secret: Secret<Vec<u8>>,
    /// Normalised `scheme://host[:port]` origins, used for CORS and the WebSocket Origin check.
    pub allowed_origins: Vec<String>,
    pub cookie_secure: bool,
    pub snapshot_interval_secs: u64,
    pub version_interval_secs: u64,
    pub auto_version_retention: u32,
    /// Bearer token for `GET /metrics`. Unset: the endpoint answers 404.
    pub metrics_token: Option<Secret<String>>,
}

/// Everything that is wrong with the environment, one message per problem. The messages name
/// variables and rules, never values.
#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError {
    pub problems: Vec<String>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, problem) in self.problems.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "  - {problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

/// The variables this module reads.
const VARIABLES: [&str; 11] = [
    "PORT",
    "MONGODB_URI",
    "MONGODB_DB",
    "JWT_SECRET",
    "TICKET_SECRET",
    "ALLOWED_ORIGINS",
    "COOKIE_SECURE",
    "SNAPSHOT_INTERVAL_SECS",
    "VERSION_INTERVAL_SECS",
    "AUTO_VERSION_RETENTION",
    "METRICS_TOKEN",
];

impl Config {
    /// Reads the process environment.
    pub fn from_env() -> Result<Self, ConfigError> {
        let vars = VARIABLES
            .iter()
            .filter_map(|name| {
                std::env::var(name)
                    .ok()
                    .map(|value| ((*name).to_string(), value))
            })
            .collect();
        Self::from_map(&vars)
    }

    /// Builds the configuration from a map. Pure, so every rule is unit-tested.
    pub fn from_map(vars: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let mut problems = Vec::new();
        let get = |name: &str| {
            vars.get(name)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
        };

        let mongodb_uri = match get("MONGODB_URI") {
            Some(uri) => Some(uri.to_string()),
            None => {
                problems.push("MONGODB_URI is required".to_string());
                None
            }
        };
        let mongodb_db = get("MONGODB_DB").unwrap_or("syncspace").to_string();

        let jwt_secret = secret(&mut problems, "JWT_SECRET", get("JWT_SECRET"));
        let ticket_secret = secret(&mut problems, "TICKET_SECRET", get("TICKET_SECRET"));
        if let (Some(jwt), Some(ticket)) = (&jwt_secret, &ticket_secret)
            && jwt == ticket
        {
            problems.push("JWT_SECRET and TICKET_SECRET must be different".to_string());
        }

        let allowed_origins = origins(&mut problems, get("ALLOWED_ORIGINS"));

        let port = number(&mut problems, "PORT", get("PORT"), 8080u16);
        let snapshot_interval_secs = number(
            &mut problems,
            "SNAPSHOT_INTERVAL_SECS",
            get("SNAPSHOT_INTERVAL_SECS"),
            5u64,
        );
        let version_interval_secs = number(
            &mut problems,
            "VERSION_INTERVAL_SECS",
            get("VERSION_INTERVAL_SECS"),
            600u64,
        );
        let auto_version_retention = number(
            &mut problems,
            "AUTO_VERSION_RETENTION",
            get("AUTO_VERSION_RETENTION"),
            50u32,
        );
        let cookie_secure = boolean(&mut problems, "COOKIE_SECURE", get("COOKIE_SECURE"), true);

        let metrics_token = get("METRICS_TOKEN").map(|t| t.trim().to_string());
        if let Some(token) = &metrics_token
            && token.chars().count() < 16
        {
            problems.push("METRICS_TOKEN must be at least 16 characters when set".to_string());
        }

        match (mongodb_uri, jwt_secret, ticket_secret) {
            (Some(uri), Some(jwt), Some(ticket)) if problems.is_empty() => Ok(Self {
                port,
                mongodb_uri: Secret::new(uri),
                mongodb_db,
                jwt_secret: Secret::new(jwt),
                ticket_secret: Secret::new(ticket),
                allowed_origins,
                cookie_secure,
                snapshot_interval_secs,
                version_interval_secs,
                auto_version_retention,
                metrics_token: metrics_token.map(Secret::new),
            }),
            _ => Err(ConfigError { problems }),
        }
    }
}

/// A positive number, or the default when the variable is not set. Zero is refused: every
/// number here is a port, an interval or a retention count, and zero would be a mistake.
fn number<T>(problems: &mut Vec<String>, name: &str, raw: Option<&str>, default: T) -> T
where
    T: std::str::FromStr + PartialEq + Default,
{
    match raw {
        None => default,
        Some(text) => match text.parse::<T>() {
            Ok(value) if value != T::default() => value,
            _ => {
                problems.push(format!("{name} must be a positive whole number"));
                default
            }
        },
    }
}

fn boolean(problems: &mut Vec<String>, name: &str, raw: Option<&str>, default: bool) -> bool {
    match raw {
        None => default,
        Some(text) if text.eq_ignore_ascii_case("true") => true,
        Some(text) if text.eq_ignore_ascii_case("false") => false,
        Some(_) => {
            problems.push(format!("{name} must be true or false"));
            default
        }
    }
}

/// A base64 key of at least [`MIN_SECRET_BYTES`] bytes once decoded.
fn secret(problems: &mut Vec<String>, name: &str, raw: Option<&str>) -> Option<Vec<u8>> {
    let Some(text) = raw else {
        problems.push(format!("{name} is required"));
        return None;
    };
    match STANDARD.decode(text) {
        Ok(bytes) if bytes.len() >= MIN_SECRET_BYTES => Some(bytes),
        _ => {
            problems.push(format!(
                "{name} must be base64 that decodes to at least {MIN_SECRET_BYTES} bytes"
            ));
            None
        }
    }
}

/// `ALLOWED_ORIGINS`: a comma separated list of `scheme://host[:port]`. The result is
/// lower-cased, without a trailing slash and without duplicates. `*` is refused: the session
/// cookie is sent with credentials, and a wildcard origin is not allowed together with them.
fn origins(problems: &mut Vec<String>, raw: Option<&str>) -> Vec<String> {
    let Some(text) = raw else {
        problems.push("ALLOWED_ORIGINS is required".to_string());
        return Vec::new();
    };
    let mut result: Vec<String> = Vec::new();
    for entry in text.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        match normalise_origin(entry) {
            Some(origin) => {
                if !result.contains(&origin) {
                    result.push(origin);
                }
            }
            None => problems.push(
                "ALLOWED_ORIGINS entries must look like https://host or http://host:port \
                 (no path, no wildcard)"
                    .to_string(),
            ),
        }
    }
    if result.is_empty() && !problems.iter().any(|p| p.starts_with("ALLOWED_ORIGINS")) {
        problems.push("ALLOWED_ORIGINS must list at least one origin".to_string());
    }
    result
}

fn normalise_origin(entry: &str) -> Option<String> {
    let lowered = entry.to_ascii_lowercase();
    let trimmed = lowered.strip_suffix('/').unwrap_or(&lowered);
    if trimmed.contains(['*', ' ', '?', '#', '@']) {
        return None;
    }
    let uri: Uri = trimmed.parse().ok()?;
    let scheme = uri.scheme_str()?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = uri.authority()?;
    // Nothing may follow the authority: `https://a.example/path` is not an origin.
    if trimmed != format!("{scheme}://{authority}") {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 bytes of 0x01 and of 0x02, base64 encoded.
    const KEY_A: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";
    const KEY_B: &str = "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=";

    fn valid() -> HashMap<String, String> {
        [
            ("MONGODB_URI", "mongodb://user:hunter2@db.example:27017"),
            ("JWT_SECRET", KEY_A),
            ("TICKET_SECRET", KEY_B),
            ("ALLOWED_ORIGINS", "https://app.example"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    fn problems(vars: &HashMap<String, String>) -> Vec<String> {
        Config::from_map(vars).unwrap_err().problems
    }

    #[test]
    fn parses_a_complete_environment() {
        let mut vars = valid();
        vars.extend(
            [
                ("PORT", "10000"),
                ("MONGODB_DB", "prod"),
                ("COOKIE_SECURE", "FALSE"),
                ("SNAPSHOT_INTERVAL_SECS", "7"),
                ("VERSION_INTERVAL_SECS", "900"),
                ("AUTO_VERSION_RETENTION", "20"),
            ]
            .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        let config = Config::from_map(&vars).unwrap();
        assert_eq!(config.port, 10000);
        assert_eq!(config.mongodb_db, "prod");
        assert!(!config.cookie_secure);
        assert_eq!(config.snapshot_interval_secs, 7);
        assert_eq!(config.version_interval_secs, 900);
        assert_eq!(config.auto_version_retention, 20);
        assert_eq!(config.allowed_origins, vec!["https://app.example"]);
        assert_eq!(config.jwt_secret.expose().len(), 32);
    }

    #[test]
    fn applies_the_documented_defaults() {
        let config = Config::from_map(&valid()).unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.mongodb_db, "syncspace");
        assert!(config.cookie_secure, "the safe default is a secure cookie");
        assert_eq!(config.snapshot_interval_secs, 5);
        assert_eq!(config.version_interval_secs, 600);
        assert_eq!(config.auto_version_retention, 50);
    }

    #[test]
    fn reports_every_missing_required_variable_at_once() {
        let found = problems(&HashMap::new());
        for name in [
            "MONGODB_URI",
            "JWT_SECRET",
            "TICKET_SECRET",
            "ALLOWED_ORIGINS",
        ] {
            assert!(
                found.iter().any(|p| p.starts_with(name)),
                "no problem reported for {name}: {found:?}"
            );
        }
        assert_eq!(found.len(), 4);
    }

    #[test]
    fn blank_values_count_as_missing() {
        let mut vars = valid();
        vars.insert("MONGODB_URI".into(), "   ".into());
        assert_eq!(problems(&vars), vec!["MONGODB_URI is required"]);
    }

    #[test]
    fn rejects_zero_and_non_numeric_numbers() {
        for (name, bad) in [
            ("PORT", "0"),
            ("PORT", "70000"),
            ("PORT", "eight"),
            ("SNAPSHOT_INTERVAL_SECS", "0"),
            ("VERSION_INTERVAL_SECS", "-5"),
            ("AUTO_VERSION_RETENTION", "1.5"),
        ] {
            let mut vars = valid();
            vars.insert(name.into(), bad.into());
            assert_eq!(
                problems(&vars),
                vec![format!("{name} must be a positive whole number")],
                "{name}={bad}"
            );
        }
    }

    #[test]
    fn rejects_an_unreadable_cookie_flag() {
        let mut vars = valid();
        vars.insert("COOKIE_SECURE".into(), "yes".into());
        assert_eq!(problems(&vars), vec!["COOKIE_SECURE must be true or false"]);
    }

    #[test]
    fn rejects_short_and_non_base64_secrets() {
        let mut vars = valid();
        vars.insert("JWT_SECRET".into(), "c2hvcnQ=".into()); // "short"
        vars.insert("TICKET_SECRET".into(), "not base64 at all!".into());
        let found = problems(&vars);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|p| p.contains("at least 32 bytes")));
    }

    #[test]
    fn rejects_identical_secrets() {
        let mut vars = valid();
        vars.insert("TICKET_SECRET".into(), KEY_A.into());
        assert_eq!(
            problems(&vars),
            vec!["JWT_SECRET and TICKET_SECRET must be different"]
        );
    }

    #[test]
    fn normalises_origins_and_drops_duplicates() {
        let mut vars = valid();
        vars.insert(
            "ALLOWED_ORIGINS".into(),
            " HTTPS://App.Example/ , http://localhost:3000, https://app.example ".into(),
        );
        let config = Config::from_map(&vars).unwrap();
        assert_eq!(
            config.allowed_origins,
            vec!["https://app.example", "http://localhost:3000"]
        );
    }

    #[test]
    fn rejects_wildcards_paths_and_other_malformed_origins() {
        for bad in [
            "*",
            "https://*.example",
            "https://app.example/dashboard",
            "https://app.example?x=1",
            "ftp://app.example",
            "app.example",
            "https://user@app.example",
            "https://",
        ] {
            let mut vars = valid();
            vars.insert("ALLOWED_ORIGINS".into(), bad.into());
            let found = problems(&vars);
            assert_eq!(found.len(), 1, "{bad}: {found:?}");
            assert!(found[0].starts_with("ALLOWED_ORIGINS"), "{bad}");
        }
    }

    #[test]
    fn debug_output_never_contains_a_secret() {
        let config = Config::from_map(&valid()).unwrap();
        let shown = format!("{config:?}");
        for secret in ["hunter2", KEY_A, KEY_B] {
            assert!(!shown.contains(secret), "{secret} leaked into {shown}");
        }
        assert!(shown.contains("<redacted>"));
    }

    #[test]
    fn error_messages_never_contain_values() {
        let mut vars = valid();
        vars.insert("JWT_SECRET".into(), "super-secret-but-not-base64!!".into());
        let shown = Config::from_map(&vars).unwrap_err().to_string();
        assert!(!shown.contains("super-secret"), "{shown}");
    }

    #[test]
    fn the_metrics_token_is_optional_but_not_short() {
        let none = Config::from_map(&valid()).unwrap();
        assert!(none.metrics_token.is_none());
        let mut vars = valid();
        vars.insert("METRICS_TOKEN".into(), " a-long-enough-token ".into());
        let set = Config::from_map(&vars).unwrap();
        assert_eq!(set.metrics_token.unwrap().expose(), "a-long-enough-token");
        vars.insert("METRICS_TOKEN".into(), "short".into());
        assert!(problems(&vars).iter().any(|p| p.contains("METRICS_TOKEN")));
    }
}
