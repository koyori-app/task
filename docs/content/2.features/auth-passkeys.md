---
title: パスキー認証
description: WebAuthn/FIDO2 によるパスワードレス認証・デバイス認証
icon: lucide:fingerprint
---

# パスキー認証 仕様書

> ステータス: **Draft** / 作成日: 2026-05-27
> **依存: なし（`users` 基盤を拡張。[OAuth 拡張](/features/auth-oauth) と独立）**

---

## 1. 概要

WebAuthn（FIDO2）を使い、Touch ID・Face ID・セキュリティキーなどをパスキーとして登録・使用できる。

パスキーは「所持（デバイス）+ 生体認証」を組み合わせた多要素認証であるため:
- **パスキーログインは 2FA（TOTP）を免除する**（[2FA 仕様書](/features/auth-2fa) 参照）
- パスワードなしの完全ログインとして機能する
- 既存のメール/パスワードや OAuth に追加で登録でき、代替認証手段になる

実装ライブラリ: [`webauthn-rs`](https://github.com/kanidm/webauthn-rs)

---

## 2. データモデル

### `passkeys`

```rust
pub struct Model {
    pub id: Uuid,
    pub user_id: Uuid,
    pub credential_id: Vec<u8>,        // WebAuthn credential ID（BYTEA）
    pub public_key: Vec<u8>,           // COSE 公開鍵（BYTEA）
    pub aaguid: Option<Vec<u8>>,       // 認証器モデル識別子（16 バイト）
    pub sign_count: i64,               // リプレイ攻撃防止用カウンター
    pub name: String,                  // ユーザーが付けた名前（例: "MacBook Touch ID"）
    pub last_used_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `credential_id` | BYTEA | NOT NULL, UNIQUE | WebAuthn が生成する認証器固有 ID |
| `public_key` | BYTEA | NOT NULL | COSE 形式の公開鍵 |
| `aaguid` | BYTEA | NULLABLE | 認証器モデル識別子（16 バイト） |
| `sign_count` | BIGINT | NOT NULL DEFAULT 0 | 署名カウンター（0 = カウンター非対応の認証器） |
| `name` | VARCHAR(255) | NOT NULL | ユーザーが付けた名前 |
| `last_used_at` | TIMESTAMPTZ | NULLABLE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> 1 ユーザーが複数デバイスにパスキーを登録できる（上限 20 個）。

---

## 3. マイグレーション

```sql
CREATE TABLE passkeys (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    credential_id BYTEA NOT NULL UNIQUE,
    public_key BYTEA NOT NULL,
    aaguid BYTEA,
    sign_count BIGINT NOT NULL DEFAULT 0,
    name VARCHAR(255) NOT NULL,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_passkeys_user ON passkeys(user_id);
```

---

## 4. 登録フロー（Registration Ceremony）

```text
1. POST /v1/auth/passkeys/registration/start
   ← ユーザーはセッション済み（既存アカウントにパスキーを追加）
   → サーバーが PublicKeyCredentialCreationOptions を生成
   → challenge を Redis に保存（TTL: 5 分）

2. フロントエンドが navigator.credentials.create(options) を呼び出す
   → ユーザーが Touch ID / Face ID / セキュリティキーで認証

3. POST /v1/auth/passkeys/registration/finish
   → サーバーが challenge 検証・署名検証・公開鍵検証
   → passkeys テーブルに INSERT
   → Redis の challenge を削除
```

`POST /auth/passkeys/registration/start` レスポンス例:

```json
{
  "challenge": "base64url-encoded-challenge",
  "rp": { "name": "TaskApp", "id": "app.example.com" },
  "user": { "id": "base64url-user-id", "name": "user@example.com", "displayName": "Alice" },
  "pubKeyCredParams": [
    { "type": "public-key", "alg": -7 },
    { "type": "public-key", "alg": -257 }
  ],
  "authenticatorSelection": {
    "residentKey": "preferred",
    "userVerification": "required"
  },
  "timeout": 60000,
  "excludeCredentials": ["...existing credential IDs..."]
}
```

`POST /auth/passkeys/registration/finish` リクエスト:

```json
{
  "name": "MacBook Touch ID",
  "credential": { "...PublicKeyCredential JSON..." }
}
```

---

## 5. 認証フロー（Authentication Ceremony）

セッションなしでパスキーだけでログインできる。

```text
1. POST /v1/auth/passkeys/authentication/start  （公開エンドポイント）
   Request: { "email": "user@example.com" }  （省略可：Conditional UI 用）
   → サーバーが PublicKeyCredentialRequestOptions を生成
   → challenge を Redis に保存（TTL: 5 分）

2. フロントエンドが navigator.credentials.get(options) を呼び出す

3. POST /v1/auth/passkeys/authentication/finish  （公開エンドポイント）
   → サーバーが challenge・署名・sign_count を検証
   → sign_count が DB の値以下なら 401（リプレイ攻撃）
   → sign_count を UPDATE
   → last_used_at を UPDATE
   → Redis セッション発行（2FA スキップ）
   → 200 OK
```

`POST /auth/passkeys/authentication/start` レスポンス例:

```json
{
  "challenge": "base64url-encoded-challenge",
  "rpId": "app.example.com",
  "allowCredentials": [
    { "type": "public-key", "id": "base64url-credential-id" }
  ],
  "userVerification": "required",
  "timeout": 60000
}
```

### Conditional UI（パスワード欄のオートフィル）

`email` を省略して `start` を呼び出すと `allowCredentials: []` を返す。  
フロントエンドは `mediation: "conditional"` を指定して `credentials.get()` を呼び出し、  
ブラウザがパスワードオートフィルの UI にパスキーを自動表示する。

---

## 6. API

| メソッド | パス | 認証 | 説明 |
|---------|------|------|------|
| `POST` | `/v1/auth/passkeys/registration/start` | セッション必須 | 登録チャレンジ発行 |
| `POST` | `/v1/auth/passkeys/registration/finish` | セッション必須 | 登録完了・DB 保存 |
| `POST` | `/v1/auth/passkeys/authentication/start` | 不要 | 認証チャレンジ発行 |
| `POST` | `/v1/auth/passkeys/authentication/finish` | 不要 | 認証検証・セッション発行 |
| `GET` | `/v1/auth/passkeys` | セッション必須 | 登録済みパスキー一覧 |
| `PATCH` | `/v1/auth/passkeys/{id}` | セッション必須 | 名前変更 |
| `DELETE` | `/v1/auth/passkeys/{id}` | セッション必須 | 削除（最後の手段ガード付き） |

`GET /v1/auth/passkeys` レスポンス:

```json
{
  "passkeys": [
    {
      "id": "uuid",
      "name": "MacBook Touch ID",
      "last_used_at": "2026-05-27T10:00:00Z",
      "created_at": "2026-05-01T09:00:00Z"
    }
  ]
}
```

---

## 7. セキュリティ

| 項目 | 仕様 |
|------|------|
| RP ID | 本番: `app.example.com`（サブドメイン可） |
| userVerification | `required`（生体認証またはデバイス PIN を必須） |
| residentKey | `preferred`（discoverable credential を推奨） |
| sign_count | 0 以外の場合は厳密チェック。0 は「非対応認証器」として許容 |
| origin 検証 | `webauthn-rs` が `https://app.example.com` のみ許可 |
| challenge TTL | Redis で 5 分。使用後即削除 |
| 最後の手段削除 | `password_hash IS NULL AND oauth_connections = 0 AND passkeys.count = 1` なら 403 |

---

## 8. フロントエンド（Phase B）

### ログイン画面への追加

```text
┌─────────────────────────────────────────────┐
│ ログイン                                     │
├─────────────────────────────────────────────┤
│ メールアドレス [___________________] ← Conditional UI でパスキー候補表示
│ パスワード     [___________________]         │
│                             [ログイン]       │
│                                             │
│ [  パスキーでログイン  ]                      │
│ [  GitHub でログイン   ]                     │
└─────────────────────────────────────────────┘
```

### セキュリティ設定画面

```text
/settings/security
```

```text
┌─────────────────────────────────────────────┐
│ パスキー                                     │
├─────────────────────────────────────────────┤
│  MacBook Touch ID    最終使用: 今日  [削除]  │
│  iPhone Face ID      最終使用: 昨日  [削除]  │
│                                             │
│  [+ パスキーを追加]                          │
└─────────────────────────────────────────────┘
```

| コンポーネント | ファイル |
|--------------|---------|
| `PasskeyLoginButton` | `components/auth/PasskeyLoginButton.vue` |
| `PasskeyManager` | `components/settings/PasskeyManager.vue` |
