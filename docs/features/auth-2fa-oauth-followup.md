---
title: OAuth ログイン × 2FA 統合（フォローアップ）
description: OAuth コールバックの half_authed はバックエンドで実装済みで、戻り先の画面が未実装である
---

> 実装状況: バックエンドは実装済みである。リダイレクト先の `/auth/2fa` のページが未実装で、受け入れ条件の 2 が満たされていない。

## 背景

OAuth のログインとパスワードのログインで 2FA の判定が食い違わないよう、OAuth コールバックもセッションの確立を `service::login_session::establish_login_session` に任せる（[2FA 仕様書](/features/auth-2fa) §5）。

`oauth_callback` は利用者を解決したあとにこの関数を呼び、2FA が必要なら `{フロントの URL}/auth/2fa?redirect_after=...` へリダイレクトする。

## 受け入れ条件と現状

| # | 条件 | 現状 |
|---|---|---|
| 1 | 2FA 有効ユーザーが OAuth ログインした場合、`half_authed: true` セッションになる | 実装済み。テナントの強制で TOTP の設定が要る利用者も半認証になる |
| 2 | フロントは `/auth/2fa` へリダイレクトする | **未実装**。バックエンドはリダイレクトするが、`/auth/2fa` のページが無い |
| 3 | `POST /v1/auth/2fa/verify` で完全認証に昇格する | API は実装済み。経路 1 の半認証セッションから画面で操作する手段は無い |
| 4 | 統合テストで検証する | 一部のみ。`oauth_integration.rs` の `oauth_callback_requires_2fa_setup_for_every_membership_form` が、リダイレクト先と半認証のあいだ `/v1/auth/me` が `403` になることを確かめる。OAuth を起点に `verify` まで通すテストは無い |

## 関連

- [2FA 仕様書](/features/auth-2fa) §5
- [OAuth 仕様書](/features/auth-oauth)
