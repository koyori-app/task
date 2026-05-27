---
title: OAuth 認証拡張
description: GitHub・GitLab・汎用 OIDC による既存認証基盤への OAuth ログイン追加
icon: lucide:key-round
---

# OAuth 認証拡張 仕様書

> ステータス: **Draft** / 作成日: 2026-05-27
> **依存: なし（既存の `users` + `sessions` 基盤を拡張）**

---

## 1. 概要

既存のメール + パスワード認証（Argon2id）に加え、GitHub・GitLab・汎用 OIDC（Google 等）でのログイン・新規登録を可能にする。

設計方針:
- **Authorization Code + PKCE** のみ（Implicit Flow 禁止）
- OAuth ユーザーもセッション管理は既存の Redis セッションと共通
- メール/パスワードユーザーは後から OAuth を連携できる（逆も可）
- パスワードなしの OAuth 専用ユーザーも許容する

---

## 2. データモデル変更

### 2.1 `users` テーブル変更

OAuth ユーザーはパスワードを持たないため `password_hash` を NULL 許容にする。

```sql
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;
```

> 既存の全ユーザーはパスワードハッシュを持つため、NULL 許容化はデータ上の破壊的変更ではない。  
> ログイン時にパスワードが NULL のユーザーへのパスワード認証は `401 invalid-credentials` を返す。

### 2.2 `oauth_connections`（新規）

```rust
pub struct Model {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,           // "github" | "gitlab" | "google" | "oidc:{issuer}"
    pub provider_user_id: String,   // プロバイダー側のユーザー ID
    pub provider_email: Option<String>, // プロバイダーが返したメール（参照用）
    pub access_token_enc: Option<String>,  // AES-256-GCM 暗号化
    pub refresh_token_enc: Option<String>, // AES-256-GCM 暗号化
    pub token_expires_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `provider` | VARCHAR | NOT NULL | `github` / `gitlab` / `google` / `oidc:{issuer}` |
| `provider_user_id` | VARCHAR | NOT NULL | プロバイダー側の不変ユーザー ID |
| `provider_email` | VARCHAR | NULLABLE | プロバイダーが返したメール（参照用。主キーとして使わない） |
| `access_token_enc` | TEXT | NULLABLE | AES-256-GCM 暗号化 |
| `refresh_token_enc` | TEXT | NULLABLE | AES-256-GCM 暗号化 |
| `token_expires_at` | TIMESTAMPTZ | NULLABLE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(provider, provider_user_id) | |

---

## 3. マイグレーション

```sql
ALTER TABLE users ALTER COLUMN password_hash DROP NOT NULL;

CREATE TABLE oauth_connections (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR NOT NULL,
    provider_user_id VARCHAR NOT NULL,
    provider_email VARCHAR,
    access_token_enc TEXT,
    refresh_token_enc TEXT,
    token_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_user_id)
);

CREATE INDEX idx_oauth_connections_user ON oauth_connections(user_id);
```

---

## 4. 対応プロバイダー

| プロバイダー | `provider` 値 | 方式 | 備考 |
|------------|--------------|------|------|
| GitHub | `github` | OAuth 2.0 | GitHub Apps とは別。`user:email` スコープのみ |
| GitLab.com | `gitlab` | OAuth 2.0 | `read_user` スコープ |
| Google | `google` | OIDC | `openid email profile` スコープ |
| 汎用 OIDC | `oidc:{issuer}` | OIDC Discovery | Okta・Entra ID・Keycloak 等に対応 |

プロバイダー設定は環境変数で管理する（テナント単位の設定は本仕様の対象外）:

```env
OAUTH_GITHUB_CLIENT_ID=...
OAUTH_GITHUB_CLIENT_SECRET=...
OAUTH_GITLAB_CLIENT_ID=...
OAUTH_GITLAB_CLIENT_SECRET=...
OAUTH_GOOGLE_CLIENT_ID=...
OAUTH_GOOGLE_CLIENT_SECRET=...
# 汎用 OIDC（複数設定可）
OAUTH_OIDC_ISSUER_URL=https://accounts.example.com
OAUTH_OIDC_CLIENT_ID=...
OAUTH_OIDC_CLIENT_SECRET=...
```

---

## 5. OAuth フロー（Authorization Code + PKCE）

```
1. クライアントが GET /auth/oauth/{provider} をリクエスト
2. バックエンドが生成:
   - code_verifier  (32 バイト乱数, base64url)
   - code_challenge (SHA-256(code_verifier), base64url)
   - state          (16 バイト乱数, base64url)
3. Redis に state をキー、{code_verifier, redirect_after} を値として保存 (TTL: 10 分)
4. 認可 URL へリダイレクト:
   ?response_type=code&client_id=...&scope=...
    &redirect_uri=...&state={state}&code_challenge={ch}&code_challenge_method=S256
5. ユーザーがプロバイダーで認可
6. プロバイダーが GET /auth/oauth/{provider}/callback?code={code}&state={state} へリダイレクト
7. バックエンドが state を検証 (Redis から取得して一致確認、即削除)
8. code + code_verifier でトークンエンドポイントへ POST → access_token 取得
9. プロバイダーの userinfo API を叩いてユーザー情報を取得
10. ユーザー照合・作成（§6 参照）
11. Redis セッション発行 → クライアントにセッション Cookie をセット
12. redirect_after へリダイレクト（デフォルト: /dashboard）
```

---

## 6. ユーザー照合ロジック

コールバック受信時、プロバイダーから `provider_user_id` と `provider_email` を受け取る。

```
A) oauth_connections に (provider, provider_user_id) が存在する
   → 既存ユーザーとしてログイン。access_token を更新。

B) 存在しない かつ provider_email が users.email と一致するユーザーがいる
   → "メール競合" → §6.1 参照

C) 存在しない かつ メール一致なし
   → 新規ユーザーを作成し oauth_connections も INSERT（§6.2 参照）
```

### 6.1 メール競合（ケース B）

既存のメール/パスワードユーザーと同じメールアドレスで OAuth ログインを試みた場合:

- **既存セッションあり（ログイン済み）**: oauth_connections を追加で INSERT → 連携完了
- **既存セッションなし**: `409 Conflict` を返し、フロントエンドが「このメールアドレスは既に登録されています。ログインして連携してください。」を表示

> プロバイダーが返すメールアドレスは不変ではなく、なりすまし防止のため自動マージはしない。

### 6.2 新規ユーザー作成（ケース C）

```rust
users::ActiveModel {
    id: Set(Uuid::new_v4()),
    username: Set(derive_username_from_provider(provider_info)),
    email: Set(provider_email),
    email_verified: Set(true),   // プロバイダーが検証済みとみなす
    password_hash: Set(None),    // OAuth ユーザーはパスワードなし
    bio: Set(Some(String::new())),
    avatar_url: Set(provider_info.avatar_url),
}
```

ユーザー名は `{provider_username}` をベースに、重複時は `{name}_2`, `{name}_3` ... とする。

---

## 7. アカウント連携・解除

### 連携（ログイン済みユーザーが追加 OAuth を接続する）

1. ログイン済みで `GET /auth/oauth/{provider}` をリクエスト（セッションあり）
2. コールバック受信後、現在のセッションユーザーに oauth_connections を追加
3. 同じプロバイダーが既に連携済みなら `409`

### 解除

```
DELETE /auth/oauth/connections/{provider}
```

**ガード**: 最後の認証手段を解除できない:

```
(oauth_connections が 1 件のみ) AND (password_hash IS NULL)
→ 403: パスワードを設定してから解除してください
```

---

## 8. API

| メソッド | パス | 認証 | 説明 |
|---------|------|------|------|
| `GET` | `/v1/auth/oauth/{provider}` | 不要 | 認可 URL へリダイレクト |
| `GET` | `/v1/auth/oauth/{provider}/callback` | 不要 | コールバック処理（公開エンドポイント） |
| `GET` | `/v1/auth/oauth/connections` | セッション必須 | 連携済みプロバイダー一覧 |
| `DELETE` | `/v1/auth/oauth/connections/{provider}` | セッション必須 | 連携解除 |
| `POST` | `/v1/auth/password` | セッション必須 | OAuth ユーザーがパスワードを新規設定 |

`GET /v1/auth/oauth/connections` レスポンス:

```json
{
  "connections": [
    {
      "provider": "github",
      "provider_email": "user@example.com",
      "connected_at": "2026-05-27T10:00:00Z"
    }
  ]
}
```

`POST /v1/auth/password`（OAuth ユーザーのパスワード初回設定）:

```json
{ "password": "new-password-min-8" }
```

> 既にパスワードが設定されている場合は `409`。変更は既存の `/v1/auth/change-password` を使用。

---

## 9. セキュリティ

| 脅威 | 対策 |
|------|------|
| CSRF | `state` パラメータを Redis で管理（TTL 10 分、コールバック後即削除） |
| Code Injection | PKCE (`code_challenge_method=S256`)。`code_verifier` は Redis にのみ存在 |
| Token 漏洩 | `access_token` / `refresh_token` は AES-256-GCM で暗号化して保存。API レスポンスでは返さない |
| メール偽装 | プロバイダーのメール一致でも自動マージしない。セッションなし時は `409` |
| provider_user_id なりすまし | `UNIQUE(provider, provider_user_id)` で保証。プロバイダー跨ぎで ID が衝突しても無効 |
| 最後の認証手段削除 | 解除前に password_hash と残 connections 数を確認 |

---

## 10. フロントエンド（Phase B）

### ログイン画面

```
┌─────────────────────────────────────────────┐
│ ログイン                                     │
├─────────────────────────────────────────────┤
│ メールアドレス [___________________]         │
│ パスワード     [___________________]         │
│                             [ログイン]       │
│                                             │
│ ─────────── または ────────────              │
│                                             │
│ [  GitHub でログイン  ]                      │
│ [  GitLab でログイン  ]                      │
│ [  Google でログイン  ]                      │
└─────────────────────────────────────────────┘
```

### アカウント設定「連携済みサービス」

```
/settings/account
```

```
┌─────────────────────────────────────────────┐
│ 連携済みサービス                              │
├─────────────────────────────────────────────┤
│ GitHub   user@example.com    [解除]          │
│ GitLab   —                   [連携する]      │
│ Google   —                   [連携する]      │
│                                             │
│ パスワード: 未設定            [パスワードを設定] │
└─────────────────────────────────────────────┘
```

### コンポーネント

| コンポーネント | ファイル |
|--------------|---------|
| `OAuthButtons` | `components/auth/OAuthButtons.vue` |
| `ConnectedServices` | `components/settings/ConnectedServices.vue` |

---

## 11. 未決事項

| 項目 | 内容 |
|------|------|
| テナント単位 OIDC | 企業向けに Okta 等を「テナント独自の IdP」として設定できるようにするか（SaaS 標準機能） |
| GitLab self-hosted | インスタンス URL をユーザーが入力して接続できるようにするか |
| アカウント統合 UI | メール競合時に「ログインして連携」を seamless に誘導するフロー設計 |
