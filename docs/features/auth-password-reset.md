---
title: パスワードリセット・変更
description: 忘れたパスワードのリセットフロー・ログイン中のパスワード変更
icon: lucide:key-round
---

# パスワードリセット・変更 仕様書

> ステータス: **Draft** / 作成日: 2026-05-27
> **依存: なし（`users` テーブル + Redis + メール送信基盤を使用）**

---

## 1. 概要

2 つのフローを提供する。

| フロー | 対象 | 認証 |
|--------|------|------|
| **パスワードリセット** | パスワードを忘れたユーザー | 不要（公開エンドポイント） |
| **パスワード変更** | ログイン中のユーザー | セッション必須 |

どちらも完了後に対象ユーザーの**既存セッションをすべて無効化**する。

管理者が特定ユーザー宛にリセットメールを送る操作は [サービス管理者仕様書](/features/admin) §7.1 で定義する。本仕様のトークン生成ロジックを共有する。

---

## 2. データモデル

DB への追加はなし。トークンは Redis のみに保持する（既存のメール認証と同じ方針）。

### 2.1 Redis キー設計

```text
pw_reset:t:{token}   → user_id（文字列）  TTL: TOKEN_TTL_SECS
pw_reset:u:{user_id} → token（文字列）    TTL: TOKEN_TTL_SECS
pw_reset:rl:{email}  → "1"（存在するだけで意味あり）  TTL: RATE_LIMIT_SECS
```

- `t:` / `u:` ペアはメール認証（`email_verify:`）と同じ双方向マッピング構造
- 同一ユーザーへの二重発行時は Lua スクリプトで旧トークンを原子的に無効化する
- `rl:` キーで送信レートを制限する（メールアドレス単位）
- `rl:` キーに使うメールアドレスは必ず正規化する（小文字変換 + 前後空白トリム）

### 2.2 セッション無効化のための `users` テーブル追加

```sql
ALTER TABLE users ADD COLUMN sessions_revoked_at TIMESTAMPTZ;
```

```rust
// entities/users.rs に追加
pub sessions_revoked_at: Option<DateTimeUtc>,
```

パスワードリセット・変更完了時に `now()` を設定する。`AuthUser` エクストラクタはセッション発行時刻と比較し、セッションが古ければ `401` を返す。

```rust
// session に発行時刻をミリ秒精度で保存
session.set("user_id", user.id);
session.set("issued_at_ms", Utc::now().timestamp_millis());

// AuthUser エクストラクタで確認（ミリ秒精度で比較してタイミング競合を防ぐ）
let issued_at_ms = session.get::<i64>("issued_at_ms").unwrap_or(0);
if let Some(revoked_at) = user.sessions_revoked_at {
    if issued_at_ms < revoked_at.timestamp_millis() {
        return Err(AuthError::Unauthorized);
    }
}
```

---

## 3. パスワードリセットフロー

```text
1. POST /v1/auth/password-reset/request  （公開）
   Request: { "email": "user@example.com" }
   → メールアドレスからユーザーを検索
   → 存在しない場合も 200 を返す（メール存在の有無を漏らさない）
   → レートリミット確認（RATE_LIMIT_SECS 以内に同一メールへ送信済みなら 429）
   → 32 バイトのランダムトークンを生成（URL-safe base64）
   → Redis に保存（TTL: TOKEN_TTL_SECS = 30 分）
   → リセットメールをジョブキューに投入
   → 200 OK（本文: 固定メッセージ）

2. POST /v1/auth/password-reset/verify  （公開）
   Request: { "token": "..." }
   → Redis でトークンの存在を確認（消費しない）
   → 有効: 200 OK
   → 無効・期限切れ: 404

3. POST /v1/auth/password-reset/complete  （公開）
   Request: { "token": "...", "new_password": "..." }
   → new_password をバリデーション（8 文字以上）← 先に検証してトークン消費を防ぐ
     → 不正: 400 Bad Request（トークンはまだ有効のまま）
   → Redis でトークンの存在確認（消費しない）
     → 無効・期限切れ: 400 Bad Request
   → Argon2id でハッシュを生成（消費前に計算を完了させる）
   → users.password_hash / sessions_revoked_at を UPDATE
   → personal_tokens の該当 user_id 行を revoked = true に UPDATE（同一 DB トランザクション）
   → Redis からトークンを消費（GETDEL）。Redis 失敗時も PAT 無効化はロールバックしない（warn ログ）
   → 200 OK
```

---

## 4. パスワード変更フロー（ログイン中）

```text
POST /v1/auth/password/change  （セッション必須）
Request: { "current_password": "...", "new_password": "..." }

→ 現在のパスワードを verify_password で検証
  → 不一致: 400 Bad Request（"current_password が正しくありません"）
→ new_password をバリデーション（8 文字以上）
→ Argon2id でハッシュ化
→ users.password_hash を UPDATE
→ users.sessions_revoked_at を now() に UPDATE（他のセッションは次回リクエスト時に遅延無効化）
→ 現在のセッションは即時削除（session.destroy()）
→ 200 OK（次のリクエストで再ログインを促す）
```

> OAuth のみで登録したユーザー（`password_hash IS NULL`）はこのエンドポイントを呼べない（`400`）。パスワードを新規設定したい場合はパスワードリセットフローを使う。

---

## 5. Redis トークン管理

メール認証（`utils/email_verification.rs`）と同じ Lua スクリプト構造を使用する。  
実装は `utils/password_reset.rs` に切り出す。

```rust
/// トークン有効期限（秒）。30 分。
pub const TOKEN_TTL_SECS: u64 = 30 * 60;

/// リセットメール送信のレートリミット（秒）。同一メールへの再送を制限。
pub const RATE_LIMIT_SECS: u64 = 60;

const KEY_TOKEN: &str = "pw_reset:t:";
const KEY_USER: &str  = "pw_reset:u:";
const KEY_RL:   &str  = "pw_reset:rl:";

/// トークンを Redis に原子的に保存（旧トークンは自動失効）。
pub async fn store_token(redis: &RedisConnection, user_id: Uuid, token: &str) -> Result<(), anyhow::Error>;

/// GETDEL でトークンを消費し user_id を返す。無効・期限切れは None。
pub async fn consume_token(redis: &RedisConnection, token: &str) -> Result<Option<Uuid>, anyhow::Error>;

/// レートリミット枠を取得（SET NX）。Ok(false) = 429 相当。
pub async fn try_acquire_rate_limit(redis: &RedisConnection, email: &str) -> Result<bool, anyhow::Error>;

/// トークンの存在確認のみ（消費しない）。
pub async fn token_exists(redis: &RedisConnection, token: &str) -> Result<bool, anyhow::Error>;
```

---

## 6. API

| メソッド | パス | 認証 | 説明 |
|---------|------|------|------|
| `POST` | `/v1/auth/password-reset/request` | 不要 | リセットメール送信 |
| `POST` | `/v1/auth/password-reset/verify` | 不要 | トークン有効確認（token は JSON ボディ） |
| `POST` | `/v1/auth/password-reset/complete` | 不要 | 新パスワード設定 |
| `POST` | `/v1/auth/password/change` | セッション必須 | ログイン中の変更 |

**リセットリクエスト**:

```json
POST /v1/auth/password-reset/request
{ "email": "user@example.com" }
```

レスポンス（メールが存在するかどうかに関わらず同一）:

```json
200 OK
{ "message": "入力されたメールアドレスにリセットリンクを送信しました（登録済みの場合）" }
```

**トークン確認**:

```json
POST /v1/auth/password-reset/verify
{ "token": "URL-safe-base64-token" }
```

```text
200 OK  — 有効
404     — 無効・期限切れ
```

**新パスワード設定**:

```json
POST /v1/auth/password-reset/complete
{
  "token": "URL-safe-base64-token",
  "new_password": "newpassword123"
}
```

```json
200 OK
{ "message": "パスワードをリセットしました。再度ログインしてください。" }
```

**ログイン中のパスワード変更**:

```json
POST /v1/auth/password/change
{
  "current_password": "oldpassword",
  "new_password": "newpassword123"
}
```

```json
200 OK
{ "message": "パスワードを変更しました。再度ログインしてください。" }
```

---

## 7. リセットメール

既存の `VerificationEmailJob` と同じ Apalis ジョブキュー構造で `PasswordResetEmailJob` を実装する。

メール本文（テキスト部分）:

```text
件名: パスワードリセットのご案内

以下のリンクをクリックしてパスワードをリセットしてください。
このリンクは 30 分間有効です。

https://app.example.com/auth/reset-password?token={token}

このメールに心当たりのない場合は無視してください。
アカウントへの変更は行われていません。
```

リンク先フロントエンドページ: `/auth/reset-password?token={token}`  
→ トークン確認（`GET verify`）→ 有効なら新パスワード入力フォームを表示

---

## 8. セキュリティ

| 脅威 | 対策 |
|------|------|
| メールアドレス列挙 | リクエスト結果は常に同一レスポンス（200）。タイミング攻撃対策に処理時間も均一化する |
| トークン総当たり | 32 バイト = 256 ビットのエントロピー。現実的に総当たり不可 |
| トークン再利用 | GETDEL で消費済みは即時削除。TTL 30 分で自動失効 |
| 旧セッションの悪用 | `sessions_revoked_at` で完了後の全セッションを無効化 |
| メール爆撃 | `pw_reset:rl:{email}` で 60 秒に 1 回に制限 |
| OAuth ユーザーへの誤ったパスワード変更 | `password_hash IS NULL` の場合 change エンドポイントは `400` |
| トークンのログ・キュー漏洩 | リセットトークンは Redis のみ。Apalis payload・構造化ログ・HTTP ログのクエリに載せない（運用: `apps/backend/docs/password-reset-flow.md`） |

### 8.1 観測性（構造化ログ）

成功時のみ `tracing` の `event` フィールドで記録する（値は `user_id` のみ）:

- `auth.password_reset.email_queued`
- `auth.password_reset.email_sent`
- `auth.password_reset.completed`
- `auth.password_change.completed`

enqueue 失敗は `warn!(user_id, error)`。外部 HTTP 応答は列挙防止のため 200 のまま。

---

## 9. フロントエンド（Phase B）

### ログイン画面に「パスワードをお忘れですか？」リンクを追加

```text
┌─────────────────────────────────────────────┐
│ ログイン                                     │
├─────────────────────────────────────────────┤
│ メールアドレス [___________________]         │
│ パスワード     [___________________]         │
│                  パスワードをお忘れですか？  │
│                             [ログイン]       │
└─────────────────────────────────────────────┘
```

### パスワードリセットページ

```text
/auth/forgot-password   ← メールアドレス入力
/auth/reset-password    ← トークン確認 + 新パスワード入力
```

### セキュリティ設定画面（パスワード変更）

```text
/settings/security
```

```text
┌─────────────────────────────────────────────┐
│ パスワード変更                               │
├─────────────────────────────────────────────┤
│ 現在のパスワード [___________________]       │
│ 新しいパスワード [___________________]       │
│ 確認            [___________________]       │
│                              [変更する]      │
└─────────────────────────────────────────────┘
```

| コンポーネント | ファイル |
|--------------|---------|
| `ForgotPasswordPage` | `pages/auth/forgot-password/+Page.vue` |
| `ResetPasswordPage` | `pages/auth/reset-password/+Page.vue` |
| `PasswordChangeForm` | `components/settings/PasswordChangeForm.vue` |
