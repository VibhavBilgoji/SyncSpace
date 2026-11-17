//! The versions repository (task 4.1): numbering under concurrency, retention, paging and the
//! compressed snapshot, against a real MongoDB.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::TestApp;
use mongodb::bson::oid::ObjectId;
use server::db::documents;
use server::db::versions::{self, AUTOMATIC, MANUAL, NewVersion, RESTORE, VersionError};

async fn document(app: &TestApp) -> (ObjectId, ObjectId) {
    let workspace = ObjectId::new();
    let record = server::documents::new_record(
        workspace,
        ObjectId::new(),
        "Versioned".to_string(),
        "📄".to_string(),
    )
    .unwrap();
    documents::insert(&app.db, &record).await.unwrap();
    (workspace, record.id)
}

fn new<'a>(
    workspace: ObjectId,
    doc: ObjectId,
    change_type: &'static str,
    state: &'a [u8],
) -> NewVersion<'a> {
    NewVersion {
        document_id: doc,
        workspace_id: workspace,
        change_type,
        label: (change_type == MANUAL).then(|| "named".to_string()),
        restored_from: (change_type == RESTORE).then_some(1),
        created_by: (change_type != AUTOMATIC).then(ObjectId::new),
        state,
    }
}

#[tokio::test]
async fn numbers_start_at_one_and_count_up() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    for expected in 1..=3 {
        let v = versions::insert_next(&app.db, new(workspace, doc, MANUAL, b"state"), 50)
            .await
            .unwrap();
        assert_eq!(v.version_number, expected);
        assert!(
            v.crdt_snapshot.is_none(),
            "the returned summary has no snapshot bytes"
        );
    }
}

#[tokio::test]
async fn saves_at_the_same_moment_never_share_a_number() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    let mut tasks = Vec::new();
    for _ in 0..25 {
        let db = app.db.clone();
        tasks.push(tokio::spawn(async move {
            versions::insert_next(&db, new(workspace, doc, MANUAL, b"x"), 50)
                .await
                .unwrap()
                .version_number
        }));
    }
    let mut numbers = Vec::new();
    for task in tasks {
        numbers.push(task.await.unwrap());
    }
    numbers.sort_unstable();
    assert_eq!(numbers, (1..=25).collect::<Vec<_>>());
}

#[tokio::test]
async fn only_automatic_versions_are_pruned_and_the_newest_are_kept() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    versions::insert_next(&app.db, new(workspace, doc, MANUAL, b"m"), 3)
        .await
        .unwrap(); // 1
    for _ in 0..6 {
        versions::insert_next(&app.db, new(workspace, doc, AUTOMATIC, b"a"), 3)
            .await
            .unwrap(); // 2..=7
    }
    versions::insert_next(&app.db, new(workspace, doc, RESTORE, b"r"), 3)
        .await
        .unwrap(); // 8
    let all = versions::list(&app.db, doc, 100, None).await.unwrap();
    let numbers: Vec<i32> = all.iter().map(|v| v.version_number).collect();
    assert_eq!(
        numbers,
        vec![8, 7, 6, 5, 1],
        "3 newest automatic, the manual and the restore stay"
    );
}

#[tokio::test]
async fn the_list_is_newest_first_pages_and_leaves_the_snapshot_out() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    for _ in 0..5 {
        versions::insert_next(&app.db, new(workspace, doc, MANUAL, b"s"), 50)
            .await
            .unwrap();
    }
    let first = versions::list(&app.db, doc, 2, None).await.unwrap();
    assert_eq!(
        first.iter().map(|v| v.version_number).collect::<Vec<_>>(),
        vec![5, 4]
    );
    let second = versions::list(&app.db, doc, 2, Some(4)).await.unwrap();
    assert_eq!(
        second.iter().map(|v| v.version_number).collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert!(first.iter().all(|v| v.crdt_snapshot.is_none()));
}

#[tokio::test]
async fn a_stored_snapshot_is_compressed_and_comes_back_identical() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    let state: Vec<u8> = (0..20_000u32).map(|n| (n % 7) as u8).collect();
    let saved = versions::insert_next(&app.db, new(workspace, doc, MANUAL, &state), 50)
        .await
        .unwrap();
    assert!(
        usize::try_from(saved.size_bytes).unwrap() < state.len() / 4,
        "it is compressed"
    );
    let stored = versions::get(&app.db, doc, 1).await.unwrap().unwrap();
    let bytes = stored.crdt_snapshot.unwrap().bytes;
    assert_eq!(bytes.len(), usize::try_from(stored.size_bytes).unwrap());
    assert_eq!(versions::decompress(&bytes).unwrap(), state);
    assert!(versions::get(&app.db, doc, 2).await.unwrap().is_none());
}

#[tokio::test]
async fn existing_numbers_reports_which_versions_are_still_there() {
    let app = TestApp::real().await;
    let (workspace, doc) = document(&app).await;
    for _ in 0..5 {
        versions::insert_next(&app.db, new(workspace, doc, AUTOMATIC, b"a"), 2)
            .await
            .unwrap();
    }
    let mut kept = versions::existing_numbers(&app.db, doc, &[1, 2, 3, 4, 5, 99])
        .await
        .unwrap();
    kept.sort_unstable();
    assert_eq!(kept, vec![4, 5]);
    assert!(
        versions::existing_numbers(&app.db, doc, &[])
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn a_version_of_a_missing_document_is_refused_and_versions_follow_their_document() {
    let app = TestApp::real().await;
    let err = versions::insert_next(
        &app.db,
        new(ObjectId::new(), ObjectId::new(), MANUAL, b"x"),
        50,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, VersionError::NoDocument));

    let (workspace, doc) = document(&app).await;
    versions::insert_next(&app.db, new(workspace, doc, MANUAL, b"x"), 50)
        .await
        .unwrap();
    assert!(documents::delete(&app.db, doc).await.unwrap());
    assert!(
        versions::list(&app.db, doc, 10, None)
            .await
            .unwrap()
            .is_empty(),
        "invariant 8"
    );
}
