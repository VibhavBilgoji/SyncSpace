//! Index creation against a real MongoDB (`docs/DATA_MODEL.md` section 4). Every test here
//! needs a running MongoDB and uses a database of its own.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{EXPECTED_MONGODB_MAJOR, TestDb};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc, oid::ObjectId};
use mongodb::error::{ErrorKind, WriteFailure};
use server::db::{INDEXES, ensure_indexes};

const DUPLICATE_KEY: i32 = 11000;

fn is_duplicate_key(error: &mongodb::error::Error) -> bool {
    matches!(
        error.kind.as_ref(),
        ErrorKind::Write(WriteFailure::WriteError(write)) if write.code == DUPLICATE_KEY
    )
}

async fn indexes_of(db: &mongodb::Database, collection: &str) -> Vec<mongodb::IndexModel> {
    db.collection::<Document>(collection)
        .list_indexes()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap()
}

/// The version guard of the version-alignment decision: tests must run on the MongoDB major
/// version production runs.
#[tokio::test]
async fn mongodb_major_version_matches_production() {
    let test_db = TestDb::connect().await;
    let info = test_db
        .db
        .run_command(doc! { "buildInfo": 1 })
        .await
        .unwrap();
    let version = info.get_str("version").unwrap();
    let major: i32 = version.split('.').next().unwrap().parse().unwrap();
    assert_eq!(
        major, EXPECTED_MONGODB_MAJOR,
        "these tests ran on MongoDB {version}, but Atlas free clusters run \
         {EXPECTED_MONGODB_MAJOR}.0. Use MongoDB {EXPECTED_MONGODB_MAJOR}.0 locally and in CI \
         (docs/DEPLOYMENT.md), or change them all together."
    );
}

#[tokio::test]
async fn ensure_indexes_creates_every_index_from_the_data_model() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();

    for spec in INDEXES {
        let existing = indexes_of(&test_db.db, spec.collection).await;
        let found = existing
            .iter()
            .find(|model| {
                model
                    .options
                    .as_ref()
                    .and_then(|o| o.name.as_deref())
                    .is_some_and(|name| name == spec.name())
            })
            .unwrap_or_else(|| panic!("{}.{} was not created", spec.collection, spec.name()));
        assert_eq!(found.keys, spec.key_document(), "{}", spec.name());
        let unique = found
            .options
            .as_ref()
            .and_then(|o| o.unique)
            .unwrap_or(false);
        assert_eq!(
            unique,
            spec.unique,
            "{}.{} unique flag",
            spec.collection,
            spec.name()
        );
    }
}

#[tokio::test]
async fn ensure_indexes_creates_nothing_beyond_the_data_model() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();

    let mut collections: Vec<&str> = INDEXES.iter().map(|spec| spec.collection).collect();
    collections.sort_unstable();
    collections.dedup();
    for collection in collections {
        let wanted = INDEXES
            .iter()
            .filter(|s| s.collection == collection)
            .count();
        // Every collection also has the automatic `_id_` index.
        assert_eq!(
            indexes_of(&test_db.db, collection).await.len(),
            wanted + 1,
            "{collection}"
        );
    }
}

#[tokio::test]
async fn ensure_indexes_is_idempotent() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();
    ensure_indexes(&test_db.db).await.unwrap();
    ensure_indexes(&test_db.db).await.unwrap();

    let total: usize = {
        let mut count = 0;
        for collection in [
            "users",
            "workspaces",
            "documents",
            "document_versions",
            "boards",
            "tasks",
            "audit_logs",
        ] {
            count += indexes_of(&test_db.db, collection).await.len();
        }
        count
    };
    // Ten of ours plus `_id_` on each of the seven collections.
    assert_eq!(total, INDEXES.len() + 7);
}

#[tokio::test]
async fn unique_email_index_rejects_a_second_user_with_the_same_email() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();
    let users = test_db.db.collection::<Document>("users");

    users
        .insert_one(doc! { "email": "ada@example.com", "name": "Ada" })
        .await
        .unwrap();
    let error = users
        .insert_one(doc! { "email": "ada@example.com", "name": "Someone else" })
        .await
        .unwrap_err();
    assert!(is_duplicate_key(&error), "{error}");

    // A different email is fine.
    users
        .insert_one(doc! { "email": "grace@example.com", "name": "Grace" })
        .await
        .unwrap();
}

#[tokio::test]
async fn unique_slug_index_rejects_a_second_workspace_with_the_same_slug() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();
    let workspaces = test_db.db.collection::<Document>("workspaces");

    workspaces
        .insert_one(doc! { "slug": "eng-team", "name": "Engineering" })
        .await
        .unwrap();
    let error = workspaces
        .insert_one(doc! { "slug": "eng-team", "name": "Another" })
        .await
        .unwrap_err();
    assert!(is_duplicate_key(&error), "{error}");
}

#[tokio::test]
async fn unique_version_index_rejects_a_repeated_version_number_per_document() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();
    let versions = test_db.db.collection::<Document>("document_versions");
    let (first, second) = (ObjectId::new(), ObjectId::new());

    versions
        .insert_one(doc! { "document_id": first, "version_number": 1 })
        .await
        .unwrap();
    let error = versions
        .insert_one(doc! { "document_id": first, "version_number": 1 })
        .await
        .unwrap_err();
    assert!(is_duplicate_key(&error), "{error}");

    // The same number in another document, and the next number in this one, are fine.
    versions
        .insert_one(doc! { "document_id": second, "version_number": 1 })
        .await
        .unwrap();
    versions
        .insert_one(doc! { "document_id": first, "version_number": 2 })
        .await
        .unwrap();
}

#[tokio::test]
async fn non_unique_indexes_allow_duplicates() {
    let test_db = TestDb::connect().await;
    ensure_indexes(&test_db.db).await.unwrap();
    let boards = test_db.db.collection::<Document>("boards");
    let workspace = ObjectId::new();
    boards
        .insert_one(doc! { "workspace_id": workspace, "name": "A" })
        .await
        .unwrap();
    boards
        .insert_one(doc! { "workspace_id": workspace, "name": "B" })
        .await
        .unwrap();
}

#[tokio::test]
async fn each_test_gets_a_database_of_its_own() {
    let one = TestDb::connect().await;
    let two = TestDb::connect().await;
    assert_ne!(one.name, two.name);
    assert!(one.name.starts_with("syncspace_test_"));

    one.db
        .collection::<Document>("users")
        .insert_one(doc! { "email": "only-in-one@example.com" })
        .await
        .unwrap();
    let in_two = two
        .db
        .collection::<Document>("users")
        .count_documents(doc! {})
        .await
        .unwrap();
    assert_eq!(in_two, 0);
}
