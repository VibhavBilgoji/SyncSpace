//! MongoDB connection and index creation (`docs/DATA_MODEL.md` section 4).

use std::time::Duration;

use mongodb::bson::{Document, doc};
use mongodb::options::{ClientOptions, IndexOptions};
use mongodb::{Client, Database, IndexModel};

use crate::config::Config;

/// How long the driver looks for a usable server before an operation fails. It is short so a
/// MongoDB outage shows up in `/healthz` and at start-up within seconds, not after the
/// driver's 30 s default. A `serverSelectionTimeoutMS` in the URI wins.
const DEFAULT_SERVER_SELECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// One index from `docs/DATA_MODEL.md` section 4. Keys use the Mongo convention: 1 ascending,
/// -1 descending.
#[derive(Debug, Clone, Copy)]
pub struct IndexSpec {
    pub collection: &'static str,
    pub keys: &'static [(&'static str, i32)],
    pub unique: bool,
}

impl IndexSpec {
    pub fn key_document(&self) -> Document {
        self.keys
            .iter()
            .map(|(field, direction)| ((*field).to_string(), (*direction).into()))
            .collect()
    }

    /// The name MongoDB gives the index by default, for example `document_id_1_version_number_-1`.
    pub fn name(&self) -> String {
        self.keys
            .iter()
            .map(|(field, direction)| format!("{field}_{direction}"))
            .collect::<Vec<_>>()
            .join("_")
    }
}

/// Every index the application relies on. Keep this identical to the list in
/// `docs/DATA_MODEL.md` section 4; a test checks the table against MongoDB.
pub const INDEXES: &[IndexSpec] = &[
    IndexSpec {
        collection: "users",
        keys: &[("email", 1)],
        unique: true,
    },
    IndexSpec {
        collection: "workspaces",
        keys: &[("slug", 1)],
        unique: true,
    },
    IndexSpec {
        collection: "workspaces",
        keys: &[("members.user_id", 1)],
        unique: false,
    },
    IndexSpec {
        collection: "documents",
        keys: &[("workspace_id", 1), ("updated_at", -1)],
        unique: false,
    },
    IndexSpec {
        collection: "document_versions",
        keys: &[("document_id", 1), ("version_number", -1)],
        unique: true,
    },
    IndexSpec {
        collection: "document_versions",
        keys: &[
            ("document_id", 1),
            ("change_type", 1),
            ("version_number", -1),
        ],
        unique: false,
    },
    IndexSpec {
        collection: "boards",
        keys: &[("workspace_id", 1)],
        unique: false,
    },
    IndexSpec {
        collection: "tasks",
        keys: &[("board_id", 1), ("status", 1), ("order", 1)],
        unique: false,
    },
    IndexSpec {
        collection: "tasks",
        keys: &[("workspace_id", 1), ("assignee_ids", 1)],
        unique: false,
    },
    IndexSpec {
        collection: "audit_logs",
        keys: &[("workspace_id", 1), ("timestamp", -1), ("_id", -1)],
        unique: false,
    },
];

/// Builds the client and returns the application database. The driver connects lazily, so this
/// succeeds even when MongoDB is down; call [`ping`] to find out.
pub async fn connect(config: &Config) -> Result<Database, mongodb::error::Error> {
    let mut options = ClientOptions::parse(config.mongodb_uri.expose().as_str()).await?;
    options.app_name = Some("syncspace-server".to_string());
    if options.server_selection_timeout.is_none() {
        options.server_selection_timeout = Some(DEFAULT_SERVER_SELECTION_TIMEOUT);
    }
    let client = Client::with_options(options)?;
    Ok(client.database(&config.mongodb_db))
}

/// Round trip to the server.
pub async fn ping(db: &Database) -> Result<(), mongodb::error::Error> {
    db.run_command(doc! { "ping": 1 }).await.map(|_| ())
}

/// Creates every index in [`INDEXES`]. Safe to run on every start: creating an index that
/// already exists with the same definition does nothing.
pub async fn ensure_indexes(db: &Database) -> Result<(), mongodb::error::Error> {
    for spec in INDEXES {
        let mut options = IndexOptions::default();
        if spec.unique {
            options.unique = Some(true);
        }
        let model = IndexModel::builder()
            .keys(spec.key_document())
            .options(options)
            .build();
        db.collection::<Document>(spec.collection)
            .create_index(model)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_names_follow_the_mongodb_default() {
        let spec = INDEXES
            .iter()
            .find(|s| s.collection == "document_versions" && s.unique)
            .unwrap();
        assert_eq!(spec.name(), "document_id_1_version_number_-1");
        assert_eq!(
            spec.key_document(),
            doc! { "document_id": 1, "version_number": -1 }
        );
    }

    #[test]
    fn the_table_has_ten_distinct_indexes_over_the_documented_collections() {
        assert_eq!(INDEXES.len(), 10);
        let mut names: Vec<_> = INDEXES
            .iter()
            .map(|s| format!("{}.{}", s.collection, s.name()))
            .collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 10, "duplicate index in the table");
        for spec in INDEXES {
            assert!(
                [
                    "users",
                    "workspaces",
                    "documents",
                    "document_versions",
                    "boards",
                    "tasks",
                    "audit_logs"
                ]
                .contains(&spec.collection),
                "{} is not a collection in docs/DATA_MODEL.md",
                spec.collection
            );
        }
    }

    /// (collection, keys, unique)
    type IndexTuple = (String, Vec<(String, i32)>, bool);

    /// Reads the index list out of `docs/DATA_MODEL.md` section 4.
    fn documented_indexes() -> Vec<IndexTuple> {
        let doc = include_str!("../../../docs/DATA_MODEL.md");
        let section = doc.find("## 4. Indexes").unwrap();
        let start = doc[section..]
            .find(
                "```
",
            )
            .unwrap()
            + section
            + 4;
        let end = doc[start..].find("```").unwrap() + start;

        let mut found = Vec::new();
        let mut collection = String::new();
        for line in doc[start..end].lines() {
            let rest = match line.split_once(':') {
                Some((name, rest)) if !line.starts_with(' ') => {
                    collection = name.trim().to_string();
                    rest
                }
                _ => line,
            };
            for segment in rest.split(';').filter(|s| s.contains('{')) {
                let open = segment.find('{').unwrap();
                let close = segment.find('}').unwrap();
                let keys = segment[open + 1..close]
                    .split(',')
                    .map(|pair| {
                        let (field, direction) = pair.split_once(':').unwrap();
                        (
                            field.trim().trim_matches('"').to_string(),
                            direction.trim().parse().unwrap(),
                        )
                    })
                    .collect();
                let unique = segment[close..].contains("unique");
                found.push((collection.clone(), keys, unique));
            }
        }
        found
    }

    #[test]
    fn the_index_table_matches_docs_data_model_section_4() {
        let mut documented = documented_indexes();
        let mut in_code: Vec<IndexTuple> = INDEXES
            .iter()
            .map(|spec| {
                let keys = spec
                    .keys
                    .iter()
                    .map(|(f, d)| ((*f).to_string(), *d))
                    .collect();
                (spec.collection.to_string(), keys, spec.unique)
            })
            .collect();
        documented.sort();
        in_code.sort();
        assert_eq!(
            documented, in_code,
            "docs/DATA_MODEL.md section 4 and db::INDEXES have drifted apart"
        );
        assert_eq!(
            documented.len(),
            10,
            "the parser should find all ten indexes"
        );
    }

    #[test]
    fn exactly_the_documented_indexes_are_unique() {
        let unique: Vec<_> = INDEXES
            .iter()
            .filter(|s| s.unique)
            .map(|s| format!("{}.{}", s.collection, s.name()))
            .collect();
        assert_eq!(
            unique,
            vec![
                "users.email_1",
                "workspaces.slug_1",
                "document_versions.document_id_1_version_number_-1"
            ]
        );
    }
}
