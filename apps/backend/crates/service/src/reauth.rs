//! 直近の本人確認（再認証）の印をセッションに持つ。
//!
//! パスキーの追加と削除は、セッション cookie だけでは通さず、直近の本人確認を求める
//! （cookie を盗んだ者が、自分の鍵を足して居座ることを防ぐ）。
//!
//! 印は、資格情報で本人と確かめた時だけ立てる。パスワードのログイン、2FA の検証、
//! パスキーのログイン、OAuth のログイン（連携の追加ではない）、再認証の API である。
//! 連携の追加や 2FA の登録では立てない。盗まれたセッションで、自分の連携や 2FA を足して
//! 印を手に入れられてしまうため。

use axum_session::Session;
use axum_session_redispool::SessionRedisPool;
use chrono::Utc;

use crate::auth::AuthError;

/// セッションに持つ印の鍵（本人確認をした時刻、ミリ秒）。
pub const REAUTH_SESSION_KEY: &str = "reauthed_at_ms";

/// 時計のずれの許容（ミリ秒）。複数の API が別々の時計を持つ場合に、立てた直後の印を拒まないため。
const CLOCK_SKEW_MS: i64 = 5_000;

/// 印の時刻が、有効期間の内か。印が無いか、期間を過ぎたか、未来に大きくずれていれば `false`。
pub fn is_fresh(reauthed_at_ms: Option<i64>, now_ms: i64, window_secs: u64) -> bool {
    let Some(at) = reauthed_at_ms else {
        return false;
    };
    let window_ms = i64::try_from(window_secs)
        .unwrap_or(i64::MAX)
        .saturating_mul(1000);
    let age_ms = now_ms.saturating_sub(at);
    age_ms >= -CLOCK_SKEW_MS && age_ms <= window_ms
}

/// 今、本人確認をしたとセッションに残す。
pub fn mark_reauthed(session: &Session<SessionRedisPool>) {
    session.set(REAUTH_SESSION_KEY, Utc::now().timestamp_millis());
}

/// 直近の本人確認が有効期間の内でなければ `ReauthRequired`（403）を返す。
pub fn require_recent_auth(
    session: &Session<SessionRedisPool>,
    window_secs: u64,
) -> Result<(), AuthError> {
    let at = session.get::<i64>(REAUTH_SESSION_KEY);
    if is_fresh(at, Utc::now().timestamp_millis(), window_secs) {
        Ok(())
    } else {
        Err(AuthError::ReauthRequired)
    }
}

/// 印が有効期間の内なら、期間の切れる時刻（ミリ秒）。内でなければ `None`。
pub fn fresh_until(reauthed_at_ms: Option<i64>, now_ms: i64, window_secs: u64) -> Option<i64> {
    if !is_fresh(reauthed_at_ms, now_ms, window_secs) {
        return None;
    }
    let window_ms = i64::try_from(window_secs)
        .unwrap_or(i64::MAX)
        .saturating_mul(1000);
    reauthed_at_ms.map(|at| at.saturating_add(window_ms))
}

/// セッションの印が有効期間の内なら、期間の切れる時刻（ミリ秒）。画面が操作の前に再認証の要否を問うために使う。
pub fn recent_auth_until(session: &Session<SessionRedisPool>, window_secs: u64) -> Option<i64> {
    fresh_until(
        session.get::<i64>(REAUTH_SESSION_KEY),
        Utc::now().timestamp_millis(),
        window_secs,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    #[test]
    fn no_mark_is_not_fresh() {
        assert!(!is_fresh(None, NOW, 300));
    }

    #[test]
    fn mark_inside_the_window_is_fresh() {
        assert!(is_fresh(Some(NOW), NOW, 300));
        assert!(is_fresh(Some(NOW - 299_000), NOW, 300));
        assert!(is_fresh(Some(NOW - 300_000), NOW, 300));
    }

    #[test]
    fn mark_past_the_window_is_not_fresh() {
        assert!(!is_fresh(Some(NOW - 300_001), NOW, 300));
        assert!(!is_fresh(Some(NOW - 3_600_000), NOW, 300));
    }

    #[test]
    fn small_clock_skew_into_the_future_is_tolerated_but_a_large_one_is_not() {
        assert!(is_fresh(Some(NOW + 4_000), NOW, 300));
        assert!(!is_fresh(Some(NOW + 6_000), NOW, 300));
    }

    #[test]
    fn zero_window_accepts_only_the_present() {
        assert!(is_fresh(Some(NOW), NOW, 0));
        assert!(!is_fresh(Some(NOW - 1), NOW, 0));
    }

    #[test]
    fn huge_window_does_not_overflow() {
        assert!(is_fresh(Some(NOW - 1_000_000), NOW, u64::MAX));
    }

    #[test]
    fn fresh_until_is_the_mark_plus_the_window() {
        assert_eq!(fresh_until(Some(NOW), NOW, 300), Some(NOW + 300_000));
        assert_eq!(
            fresh_until(Some(NOW - 120_000), NOW, 300),
            Some(NOW + 180_000)
        );
    }

    #[test]
    fn fresh_until_is_none_without_a_fresh_mark() {
        assert_eq!(fresh_until(None, NOW, 300), None);
        assert_eq!(fresh_until(Some(NOW - 300_001), NOW, 300), None);
        assert_eq!(fresh_until(Some(NOW + 6_000), NOW, 300), None);
    }

    #[test]
    fn fresh_until_saturates_with_a_huge_window() {
        assert_eq!(fresh_until(Some(NOW), NOW, u64::MAX), Some(i64::MAX));
    }
}
