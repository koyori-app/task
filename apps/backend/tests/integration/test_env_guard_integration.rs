//! 試験の行き先の守り（`common::check_test_endpoint`）。
//!
//! 統合テストは試験用と明示した行き先にだけ繋ぐ。loopback でも Unix socket でもない
//! 行き先は、`TEST_ALLOW_NON_LOOPBACK=1` が無い限り繋ぐ前に拒む。

use crate::common::{check_test_endpoint, slot_database, slot_redis_url};

#[test]
fn loopback_and_unix_socket_endpoints_are_accepted() {
    for url in [
        "postgresql://test:test@localhost:5432/test",
        "postgresql://postgres@127.0.0.1:38909/task_test",
        "postgresql://postgres@[::1]:5432/task_test",
        "postgresql:///task_test?host=/var/run/postgresql&user=postgres",
        "redis://127.0.0.1:6379",
        "redis://localhost:6379/1",
    ] {
        assert_eq!(check_test_endpoint(url, false), Ok(()), "{url}");
    }
}

#[test]
fn non_loopback_endpoints_are_refused_without_the_flag() {
    // 203.0.113.0/24 は文書用の address（RFC 5737）。
    for url in [
        "postgresql://test:secret@203.0.113.10:5432/test",
        "postgresql://test:secret@db.example.com:5432/test",
        "postgresql://test:secret@localhost:5432/test?host=203.0.113.10",
        "redis://:secret@203.0.113.11:6379",
        "postgresql://test:secret@postgres:5432/test",
    ] {
        let err = check_test_endpoint(url, false).expect_err(url);
        assert!(err.contains("non-loopback"), "{url}: {err}");
        assert!(err.contains("TEST_ALLOW_NON_LOOPBACK=1"), "{url}: {err}");
        // 資格を含みうる URL そのものは、誤りの文に載せない。
        assert!(!err.contains("secret"), "{url}: {err}");
    }
}

#[test]
fn non_loopback_endpoints_are_accepted_with_the_flag() {
    // CI のサービスコンテナ（サービス名で届く）は、旗で明示して許す。
    for url in [
        "postgresql://test:test@postgres:5432/test",
        "redis://redis:6379",
    ] {
        assert_eq!(check_test_endpoint(url, true), Ok(()), "{url}");
    }
}

#[test]
fn malformed_endpoints_are_refused() {
    assert!(check_test_endpoint("not a url", true).is_err());
}

#[test]
fn slot_database_derives_template_and_slot_names() {
    let names = slot_database("postgresql://test:test@localhost:5432/test", 3).expect("names");
    assert_eq!(names.template, "test_tmpl");
    assert_eq!(names.database, "test_slot_3");
    assert_eq!(
        names.url,
        "postgresql://test:test@localhost:5432/test_slot_3"
    );

    // クエリ（libpq の形の行き先）は保つ
    let names = slot_database(
        "postgresql:///task_test?host=/var/run/postgresql&user=postgres",
        0,
    )
    .expect("names");
    assert_eq!(
        names.url,
        "postgresql:///task_test_slot_0?host=/var/run/postgresql&user=postgres"
    );
}

#[test]
fn slot_database_refuses_names_it_cannot_derive_from() {
    for url in [
        "postgresql://test:test@localhost:5432",
        "postgresql://test:test@localhost:5432/",
        "postgresql://test:test@localhost:5432/my-db",
        "postgresql://test:test@localhost:5432/a%22b",
        "postgresql://test:test@localhost:5432/a/b",
    ] {
        assert!(slot_database(url, 0).is_err(), "{url}");
    }

    // Postgres は 63 バイトを超えた識別子を黙って切り詰める（枠どうしが同じ名になる）。
    // 一番長い派生名 `<db>_slot_<N>` で測る。
    let url = |len: usize| format!("postgresql://test@localhost/{}", "d".repeat(len));
    let longest = slot_database(&url(63 - "_slot_15".len()), 15).expect("63 bytes fit");
    assert_eq!(longest.database.len(), 63);
    assert!(slot_database(&url(64 - "_slot_15".len()), 15).is_err());
    // 桁が増えて 64 バイトになる枠も拒む
    assert!(slot_database(&url(63 - "_slot_9".len()), 9).is_ok());
    assert!(slot_database(&url(63 - "_slot_9".len()), 10).is_err());
}

#[test]
fn slot_redis_url_selects_a_database_per_slot() {
    assert_eq!(
        slot_redis_url("redis://127.0.0.1:6379", 0).as_deref(),
        Ok("redis://127.0.0.1:6379/0")
    );
    assert_eq!(
        slot_redis_url("redis://127.0.0.1:6379", 15).as_deref(),
        Ok("redis://127.0.0.1:6379/15")
    );
    assert_eq!(
        slot_redis_url("redis://localhost:6379/1", 4).as_deref(),
        Ok("redis://localhost:6379/4")
    );
    // Valkey の DB は 16 個（0〜15）
    let err = slot_redis_url("redis://127.0.0.1:6379", 16).expect_err("no 17th database");
    assert!(err.contains("--test-threads"), "{err}");
}
