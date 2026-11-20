//! 試験の行き先の守り（`common::check_test_endpoint`）。
//!
//! 統合テストは試験用と明示した行き先にだけ繋ぐ。loopback でも Unix socket でもない
//! 行き先は、`TEST_ALLOW_NON_LOOPBACK=1` が無い限り繋ぐ前に拒む。

use crate::common::{check_test_endpoint, nextest_slot_from, slot_database, slot_redis_url};

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
    // Valkey の DB は 16 個（0〜15）
    let err = slot_redis_url("redis://127.0.0.1:6379", 16).expect_err("no 17th database");
    assert!(err.contains("--test-threads"), "{err}");
    assert!(slot_redis_url("redis://127.0.0.1:6379/main", 0).is_err());
}

/// 渡された DB 番号が起点になる。同じ Valkey へ向けた 2 つの run は、起点を変えれば重ならない。
#[test]
fn slot_redis_url_counts_from_the_given_database() {
    assert_eq!(
        slot_redis_url("redis://localhost:6379/1", 4).as_deref(),
        Ok("redis://localhost:6379/5")
    );
    // `/0` を 8 並列（0〜7）と `/8` を 8 並列（8〜15）は重ならない
    let first: Vec<_> = (0..8)
        .map(|slot| slot_redis_url("redis://localhost:6379/0", slot).expect("first run"))
        .collect();
    let second: Vec<_> = (0..8)
        .map(|slot| slot_redis_url("redis://localhost:6379/8", slot).expect("second run"))
        .collect();
    assert_eq!(second[7], "redis://localhost:6379/15");
    assert!(first.iter().all(|url| !second.contains(url)));
    // 起点 8 の 9 並列目は範囲の外
    let err = slot_redis_url("redis://localhost:6379/8", 8).expect_err("out of range");
    assert!(err.contains("`--test-threads` を 8 以下"), "{err}");
}

/// Unix socket の行き先はパスが socket の場所なので、パスを保って `db` で分ける。
#[test]
fn slot_redis_url_keeps_the_unix_socket_path() {
    assert_eq!(
        slot_redis_url("redis+unix:///run/valkey.sock", 3).as_deref(),
        Ok("redis+unix:///run/valkey.sock?db=3")
    );
    assert_eq!(
        slot_redis_url("unix:///run/valkey.sock?db=8&pass=x", 2).as_deref(),
        Ok("unix:///run/valkey.sock?pass=x&db=10")
    );
    assert!(slot_redis_url("redis+unix:///run/valkey.sock?db=8", 8).is_err());
}

#[test]
fn nextest_slot_is_required_under_nextest_and_absent_under_cargo_test() {
    let run_id = "7bcb76bb-b797-4168-8122-39a7783c2ba3";
    assert_eq!(
        nextest_slot_from(true, Some("3"), Some(run_id)),
        Ok(Some((3, run_id.to_string())))
    );
    // `cargo test` は枠を持たない（1 つの DB を共有する直列の経路）
    assert_eq!(nextest_slot_from(false, None, None), Ok(None));
    assert_eq!(nextest_slot_from(false, Some("3"), Some(run_id)), Ok(None));
    // nextest の下で枠が読めないのに共有のまま進むと、並列の試験が DDL でぶつかる。止める
    for (slot, id) in [
        (None, Some(run_id)),
        (Some("x"), Some(run_id)),
        (Some("3"), None),
        (Some("3"), Some("")),
        (Some("3"), Some("a'; DROP DATABASE x; --")),
    ] {
        let err = nextest_slot_from(true, slot, id).expect_err("unreadable slot");
        assert!(err.contains("0.9.90"), "{err}");
    }
}
