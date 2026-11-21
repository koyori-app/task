//! 試験ごとの DB の隔離（`common::per_test_database`）。
//!
//! nextest に `TEST_DATABASE_URL` を渡すと、ハーネスは試験ごとに雛形から専用の DB を作り直す。
//! どの試験も、他の試験が残した行の無い DB から始まる。

use crate::common::{TestApp, per_test_database};
use entity::users;
use sea_orm::{EntityTrait, PaginatorTrait};

/// 空の DB から始まることを確かめ、次の試験に見えるはずの行を 1 つ残す。
async fn starts_empty_then_leaves_a_user() {
    let app = TestApp::new().await;
    // `cargo test` と、行き先を渡さない nextest は、DB を試験ごとに作り直さない
    // （前者は 1 つの DB を共有し、後者はコンテナごと専用）。ここで確かめる隔離の対象ではない。
    if !per_test_database() {
        return;
    }
    let before = users::Entity::find()
        .count(&app.state.db)
        .await
        .expect("count users");
    assert_eq!(before, 0, "他の試験が残した行が見えている");
    app.insert_user_default().await;
}

#[tokio::test]
async fn each_test_starts_from_an_empty_database_a() {
    starts_empty_then_leaves_a_user().await;
}

#[tokio::test]
async fn each_test_starts_from_an_empty_database_b() {
    starts_empty_then_leaves_a_user().await;
}
