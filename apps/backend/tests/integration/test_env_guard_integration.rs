//! 試験の行き先の守り（`common::check_test_endpoint`）。
//!
//! 統合テストは試験用と明示した行き先にだけ繋ぐ。loopback でも Unix socket でもない
//! 行き先は、`TEST_ALLOW_NON_LOOPBACK=1` が無い限り繋ぐ前に拒む。

use crate::common::check_test_endpoint;

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
