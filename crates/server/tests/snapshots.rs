//! Integration tests for snapshots and metrics (task 2.4): the worker, the final flush when the
//! last socket leaves, the flush on shutdown, `SNAPSHOT_SAVED` and `GET /metrics`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::time::Duration;

use common::ws::WsClient;
use common::{Account, TestApp, TestWorkspace};
use mongodb::bson::oid::ObjectId;
use server::db::documents;
use server::rbac::Role;
use sync_crdt::Document;

struct Fixture<'a> {
    workspace: TestWorkspace<'a>,
    doc_id: String,
    doc: ObjectId,
}

impl<'a> Fixture<'a> {
    async fn new(app: &'a TestApp) -> Self {
        let workspace = TestWorkspace::new(app)
            .await
            .with_member(Role::Editor)
            .await;
        let record = server::documents::new_record(
            workspace.id,
            workspace.owner.id,
            "Notes".to_string(),
            "📄".to_string(),
        )
        .unwrap();
        documents::insert(&app.db, &record).await.unwrap();
        Self {
            doc_id: record.id.to_hex(),
            doc: record.id,
            workspace,
        }
    }

    fn editor(&self) -> &Account {
        self.workspace.member(Role::Editor)
    }

    async fn join(&self, app: &TestApp, client_id: u64) -> WsClient {
        let mut client = WsClient::open(app, self.editor(), &self.doc_id, client_id).await;
        client.handshake().await;
        client
    }

    /// What MongoDB holds for the document: the revision, the projected text of the first block,
    /// and the text a replica rebuilt from the stored snapshot has.
    async fn stored(&self, app: &TestApp) -> (i64, String, String) {
        let record = documents::find_with_snapshot(&app.db, self.doc)
            .await
            .unwrap()
            .unwrap();
        let replica = Document::new(77).unwrap();
        replica
            .apply_update(&record.crdt_snapshot.unwrap().bytes)
            .unwrap();
        let first = replica.blocks()[0].block_id.clone();
        (
            record.revision,
            record
                .blocks
                .first()
                .map(|b| b.content.clone())
                .unwrap_or_default(),
            replica.block_content(&first).unwrap(),
        )
    }
}

async fn type_text(client: &mut WsClient, text: &str) {
    let first = client.doc.blocks()[0].block_id.clone();
    let at = client
        .doc
        .block_content(&first)
        .unwrap()
        .encode_utf16()
        .count();
    let update = client.doc.insert_text(&first, at as u32, text).unwrap();
    client.send_update(&update).await;
}

async fn wait_until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    for _ in 0..100 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

#[tokio::test]
async fn the_worker_writes_a_changed_document_and_tells_the_sockets() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let worker =
        server::hub::snapshot::spawn(app.hub.clone(), app.db.clone(), Duration::from_millis(100));
    let mut client = fx.join(&app, 1).await;
    assert_eq!(
        fx.stored(&app).await.0,
        0,
        "a new document is at revision 0"
    );

    type_text(&mut client, "Hello").await;
    let saved = client.recv_json("SNAPSHOT_SAVED").await;
    assert_eq!(saved["revision"], 1);
    assert!(saved["saved_at"].as_str().unwrap().ends_with('Z'));
    let (revision, projected, rebuilt) = fx.stored(&app).await;
    assert_eq!(
        (revision, projected.as_str(), rebuilt.as_str()),
        (1, "Hello", "Hello")
    );

    // Nothing changed since: the next ticks write nothing.
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert_eq!(fx.stored(&app).await.0, 1);
    assert_eq!(app.hub.metrics.snapshot_writes(), 1);

    // A second edit is a second write, revision 2.
    type_text(&mut client, " world").await;
    assert_eq!(client.recv_json("SNAPSHOT_SAVED").await["revision"], 2);
    assert_eq!(fx.stored(&app).await.1, "Hello world");
    worker.abort();
}

#[tokio::test]
async fn many_edits_in_one_interval_make_one_write() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 2).await;
    for n in 0..20 {
        type_text(&mut client, &n.to_string()).await;
    }
    let room = app.hub.room(&app.db, fx.doc).await.unwrap();
    wait_until("the edits to be applied", || async {
        app.hub.metrics.edits() >= 20
    })
    .await;
    assert!(room.is_dirty());
    assert!(app.hub.flush_room(&app.db, &room).await.unwrap());
    assert!(
        !app.hub.flush_room(&app.db, &room).await.unwrap(),
        "nothing left to write"
    );
    assert_eq!(app.hub.metrics.snapshot_writes(), 1, "20 edits, one write");
    assert_eq!(app.hub.metrics.edits(), 20);
    assert_eq!(fx.stored(&app).await.0, 1);
}

#[tokio::test]
async fn the_last_socket_leaving_writes_the_document_and_forgets_the_room() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut first = fx.join(&app, 3).await;
    let second = fx.join(&app, 4).await;
    type_text(&mut first, "kept").await;
    wait_until("the update to reach the second client", || async {
        app.hub.metrics.edits() >= 1
    })
    .await;

    // One leaves: the other is still there, so nothing is written and the room stays.
    first.close().await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(app.hub.open_rooms(), 1);
    assert_eq!(fx.stored(&app).await.0, 0);

    // The last one leaves: written, and the room is gone from memory.
    second.close().await;
    wait_until("the room to be evicted", || async {
        app.hub.open_rooms() == 0
    })
    .await;
    let (revision, projected, rebuilt) = fx.stored(&app).await;
    assert_eq!(
        (revision, projected.as_str(), rebuilt.as_str()),
        (1, "kept", "kept")
    );

    // Coming back loads it again from MongoDB.
    let again = fx.join(&app, 5).await;
    let first_block = again.doc.blocks()[0].block_id.clone();
    assert_eq!(again.doc.block_content(&first_block).unwrap(), "kept");
    assert_eq!(app.hub.open_rooms(), 1);
}

#[tokio::test]
async fn a_leave_with_nothing_changed_writes_nothing_but_still_forgets_the_room() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let client = fx.join(&app, 6).await;
    client.close().await;
    wait_until("the room to be evicted", || async {
        app.hub.open_rooms() == 0
    })
    .await;
    assert_eq!(fx.stored(&app).await.0, 0);
    assert_eq!(app.hub.metrics.snapshot_writes(), 0);
}

#[tokio::test]
async fn shutdown_writes_every_changed_document_and_sends_the_sockets_away_with_1001() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 7).await;
    type_text(&mut client, "before the stop").await;
    wait_until("the edit to be applied", || async {
        app.hub.metrics.edits() >= 1
    })
    .await;

    app.hub.shutdown(&app.db).await;
    // Checked before the socket is read: the final write of a leaving socket would also save it,
    // and this test is about `shutdown` itself.
    assert_eq!(fx.stored(&app).await.1, "before the stop");
    assert_eq!(client.recv_close().await, Some(1001));
}

#[tokio::test]
async fn a_document_deleted_while_open_is_not_written_back() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 8).await;
    type_text(&mut client, "gone").await;
    wait_until("the edit to be applied", || async {
        app.hub.metrics.edits() >= 1
    })
    .await;
    let room = app.hub.room(&app.db, fx.doc).await.unwrap();
    assert!(documents::delete(&app.db, fx.doc).await.unwrap());

    assert!(!app.hub.flush_room(&app.db, &room).await.unwrap());
    assert!(
        documents::find(&app.db, fx.doc).await.unwrap().is_none(),
        "not resurrected"
    );
}

#[tokio::test]
async fn the_metrics_page_shows_counts_and_nothing_else() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 9).await;
    type_text(&mut client, "x").await;
    wait_until("the edit to be applied", || async {
        app.hub.metrics.edits() >= 1
    })
    .await;
    let room = app.hub.room(&app.db, fx.doc).await.unwrap();
    app.hub.flush_room(&app.db, &room).await.unwrap();

    let response = app
        .client
        .get(app.url("/metrics"))
        .bearer_auth(common::METRICS_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    let body = response.text().await.unwrap();
    for expected in [
        "syncspace_edits_total 1",
        "syncspace_mongo_snapshot_writes_total 1",
        "syncspace_open_rooms 1",
        "syncspace_open_sockets 1",
        "syncspace_broadcast_seconds_count 1",
    ] {
        assert!(
            body.lines().any(|l| l == expected),
            "{expected} missing in\n{body}"
        );
    }
    assert!(!body.contains(&fx.doc_id), "no identifiers on the page");
    assert!(!body.contains("Notes"), "no titles on the page");
}

#[tokio::test]
async fn a_flush_that_is_dropped_mid_write_leaves_the_document_dirty() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 20).await;
    type_text(&mut client, "precious").await;
    wait_until("the edit to be applied", || async {
        app.hub.metrics.edits() >= 1
    })
    .await;
    let room = app.hub.room(&app.db, fx.doc).await.unwrap();

    // Polled exactly once, then dropped while the MongoDB write is pending.
    {
        use futures_util::FutureExt;
        assert!(
            app.hub.flush_room(&app.db, &room).now_or_never().is_none(),
            "the write cannot finish in a single poll"
        );
    }
    assert!(room.is_dirty(), "the edit must not be forgotten");

    assert!(app.hub.flush_room(&app.db, &room).await.unwrap());
    assert_eq!(fx.stored(&app).await.1, "precious");
}

#[tokio::test]
async fn two_flushes_at_once_write_the_latest_state_last() {
    let app = TestApp::real().await;
    let fx = Fixture::new(&app).await;
    let mut client = fx.join(&app, 21).await;
    type_text(&mut client, "one").await;
    wait_until("the first edit", || async { app.hub.metrics.edits() >= 1 }).await;
    let room = app.hub.room(&app.db, fx.doc).await.unwrap();
    let (a, b) = tokio::join!(app.hub.flush_room(&app.db, &room), async {
        type_text(&mut client, "two").await;
        wait_until("the second edit", || async { app.hub.metrics.edits() >= 2 }).await;
        app.hub.flush_room(&app.db, &room).await
    });
    a.unwrap();
    b.unwrap();
    app.hub.flush_room(&app.db, &room).await.unwrap();
    assert_eq!(fx.stored(&app).await.1, "onetwo");
}

#[tokio::test]
async fn the_metrics_page_is_a_404_without_the_right_token_or_when_none_is_set() {
    let app = TestApp::real().await;
    let url = app.url("/metrics");
    let get = |auth: Option<&'static str>| {
        let request = app.client.get(&url);
        let request = match auth {
            Some(value) => request.header("authorization", value),
            None => request,
        };
        async move { request.send().await.unwrap() }
    };
    let unknown = app
        .client
        .get(app.url("/no-such-page"))
        .send()
        .await
        .unwrap();
    let wrong = get(Some("Bearer not-the-token-at-all")).await;
    assert_eq!(wrong.status(), 404);
    assert_eq!(get(None).await.status(), 404);
    assert_eq!(
        get(Some("Basic test-metrics-token-1234")).await.status(),
        404
    );
    assert_eq!(
        get(Some("Bearer test-metrics-token-123")).await.status(),
        404,
        "a prefix is not enough"
    );
    assert_eq!(
        wrong.text().await.unwrap(),
        unknown.text().await.unwrap(),
        "same body as any unknown route"
    );
    assert_eq!(
        get(Some("Bearer test-metrics-token-1234")).await.status(),
        200
    );

    let open = TestApp::spawn(
        common::AppOptions {
            metrics_token: None,
            ..common::AppOptions::default()
        },
        |routes| routes,
    )
    .await;
    let response = open
        .client
        .get(open.url("/metrics"))
        .bearer_auth(common::METRICS_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "no token configured: the endpoint does not exist"
    );
}
