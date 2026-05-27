---
title: サービス管理者・監査ログ
description: システム管理者フラグ・管理者専用 API・全操作の監査ログ
icon: lucide:shield-alert
---

# サービス管理者・監査ログ 仕様書

> ステータス: **Draft** / 作成日: 2026-05-27
> **依存: なし（`users` + `tenants` テーブルのみ使用）**

---

## 1. 概要

本 PR は 2 つの機能を追加する。

1. **サービス管理者フラグ** — `users.is_admin` による、テナントを横断した全権管理者の識別
2. **監査ログ** — 権限付与・重要操作をすべて `audit_logs` テーブルに記録し、管理者が検索・閲覧できる

管理者（`is_admin = true`）が実行できる操作:

| カテゴリ | 操作 |
|---------|------|
| **閲覧** | 全テナント・プロジェクト・タスクの閲覧（読み取り専用） |
| **ユーザー管理** | ユーザーの作成・停止・停止解除・強制削除 |
| **ユーザー管理** | パスワードリセットリンクの生成（送信先メール指定可） |
| **ユーザー管理** | パスキー削除・2FA リセット・OAuth 連携解除 |
| **テナント管理** | テナント強制削除 |
| **システム** | システム設定変更 |
| **監査** | 監査ログ閲覧・検索 |

> **閲覧は読み取り専用**: 管理者はテナント・プロジェクト・タスクの内容を**閲覧のみ**できる。ユーザーのデータを直接編集する権限は持たない。

監査ログはサービスの透明性・セキュリティ調査・コンプライアンス対応のために設計する。

---

## 2. データモデル

### 2.1 `users` テーブルへの追加

```sql
ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE users ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT false;
```

```rust
// entities/users.rs に追加
pub is_admin: bool,
pub is_suspended: bool,
```

1 ユーザーにつき `is_admin` フラグ 1 本のみ。ロール階層は設けない（管理者か否かの 2 値）。

---

### 2.2 `audit_logs` テーブル

```rust
pub struct Model {
    pub id: Uuid,
    pub actor_id: Option<Uuid>,      // NULL = システム自動実行
    pub actor_type: String,          // "user" | "system"
    pub action: String,              // ドット記法（例: "user.admin.grant"）
    pub resource_type: String,       // "user" | "tenant" | "project" | ...
    pub resource_id: String,         // 対象リソースの ID（UUID 文字列など）
    pub tenant_id: Option<Uuid>,     // テナントスコープ操作のみ設定
    pub metadata: Option<serde_json::Value>, // before/after・理由など
    pub ip_address: Option<String>,  // IPv4 / IPv6 文字列
    pub user_agent: Option<String>,
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `actor_id` | UUID | NULLABLE, FK→users SET NULL | 操作者ユーザー。NULL = システム |
| `actor_type` | VARCHAR | NOT NULL | `user` / `system` |
| `action` | VARCHAR | NOT NULL | ドット記法アクション名（後述） |
| `resource_type` | VARCHAR | NOT NULL | 対象リソース種別 |
| `resource_id` | VARCHAR | NOT NULL | 対象リソース ID |
| `tenant_id` | UUID | NULLABLE, FK→tenants SET NULL | テナントスコープ操作のみ |
| `metadata` | JSONB | NULLABLE | 変更前後・補足情報など |
| `ip_address` | VARCHAR(45) | NULLABLE | |
| `user_agent` | VARCHAR | NULLABLE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> **監査ログは不変（append-only）**: UPDATE・DELETE 不可。物理削除は保持ポリシー（デフォルト 2 年）による自動アーカイブのみ。

---

## 3. マイグレーション

```sql
ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE users ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE audit_logs (
    id UUID PRIMARY KEY,
    actor_id UUID REFERENCES users(id) ON DELETE SET NULL,
    actor_type VARCHAR NOT NULL,
    action VARCHAR NOT NULL,
    resource_type VARCHAR NOT NULL,
    resource_id VARCHAR NOT NULL,
    tenant_id UUID REFERENCES tenants(id) ON DELETE SET NULL,
    metadata JSONB,
    ip_address VARCHAR(45),
    user_agent VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_logs_actor ON audit_logs(actor_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_logs_tenant ON audit_logs(tenant_id);
CREATE INDEX idx_audit_logs_created ON audit_logs(created_at DESC);
```

---

## 4. 初期管理者のブートストラップ

アプリ起動時に `BOOTSTRAP_ADMIN_EMAIL` 環境変数が設定されている場合、該当ユーザーの `is_admin` を `true` に設定する。

```text
起動時:
  BOOTSTRAP_ADMIN_EMAIL が設定されている
    → users WHERE is_admin = true の件数を確認
    → 管理者が 1 人以上存在する場合: ブートストラップをスキップ（ログ出力のみ）
    → 管理者が 0 人の場合のみ:
        → users WHERE email = $email → is_admin = true に UPDATE
        → audit_logs に action="user.admin.grant", actor_type="system" を INSERT
```

> **安全ガード**: 管理者が既に存在する場合はブートストラップを実行しない。これにより、運用中に環境変数が誤設定されても意図しない権限付与が発生しない。初期構築後は `BOOTSTRAP_ADMIN_EMAIL` を環境変数から削除することを推奨する。

---

## 5. AdminUser エクストラクタ

```rust
/// is_admin = true のユーザー専用エクストラクタ。セッション認証のみ受理（PAT 不可）。
pub struct AdminUser {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

        // 管理者操作はセッション認証のみ。PAT は権限範囲外として 403。
        auth.require_session().map_err(|_| AppError::Forbidden)?;

        let user = users::Entity::find_by_id(auth.user_id)
            .one(&state.db)
            .await?
            .ok_or(AppError::Unauthorized)?;

        if !user.is_admin {
            return Err(AppError::Forbidden);
        }

        Ok(AdminUser { user_id: auth.user_id })
    }
}
```

管理者専用エンドポイントはすべて `AdminUser` エクストラクタを使用する。非管理者・PAT 認証は `403 Forbidden`。

`AuthUser` エクストラクタには `is_suspended` チェックを追加する:

```rust
// extractors.rs: user_id 取得後に追加
let user = users::Entity::find_by_id(user_id).one(&state.db).await?
    .ok_or(AuthError::Unauthorized)?;
if user.is_suspended {
    return Err(AuthError::Suspended);  // → 403 Forbidden
}
```

---

## 6. 監査ログ記録対象

### 6.1 アクション一覧

#### Phase A（本 PR で必須実装）

管理者が行う操作のみを記録する。

| アクション | resource_type | トリガー |
|-----------|--------------|---------|
| `user.admin.grant` | `user` | 管理者フラグ付与 |
| `user.admin.revoke` | `user` | 管理者フラグ剥奪 |
| `user.create` | `user` | 管理者によるユーザー作成 |
| `user.suspend` | `user` | ユーザー停止 |
| `user.unsuspend` | `user` | ユーザー停止解除 |
| `user.delete` | `user` | ユーザー強制削除（管理者のみ） |
| `user.password_reset` | `user` | パスワードリセットリンク生成（管理者操作） |
| `user.2fa.reset` | `user` | 2FA 強制リセット（管理者操作） |
| `user.passkey.delete` | `user` | パスキー強制削除（管理者操作） |
| `user.oauth.disconnect` | `user` | OAuth 連携強制解除（管理者操作） |
| `tenant.delete` | `tenant` | テナント強制削除（管理者のみ） |
| `system.settings.update` | `system` | システム設定変更 |

#### Phase B（後続 PR で追加）

一般ユーザーの重要操作まで監査対象を拡大する。各イベントは対応機能の PR と同時に実装する。

| アクション | resource_type | 対応機能 |
|-----------|--------------|---------|
| `auth.login.success` / `auth.login.failure` | `user` | 認証基盤 |
| `auth.2fa.enable` / `auth.2fa.disable` | `user` | 2FA |
| `auth.passkey.add` / `auth.passkey.remove` | `user` | パスキー |
| `auth.oauth.connect` / `auth.oauth.disconnect` | `user` | OAuth |
| `auth.pat.create` / `auth.pat.delete` | `user` | PAT |
| `tenant.owner.transfer` | `tenant` | テナント管理 |
| `project.member.add` / `project.member.remove` / `project.member.role.change` | `project` | プロジェクト管理 |
| `project.delete` | `project` | プロジェクト管理 |
| `github.integration.connect` / `github.integration.disconnect` | `project` | GitHub 連携 |

### 6.2 `metadata` フィールドの構造例

```json
// user.admin.grant
{ "reason": "初期管理者設定" }

// user.password_reset
{ "reset_email": "support@example.com", "original_email": "user@example.com" }

// user.2fa.reset
{ "reason": "ユーザーがデバイスを紛失" }

// project.member.role.change
{ "before": "Member", "after": "Admin" }

// auth.login.failure
{ "email": "user@example.com", "reason": "invalid_password" }

// tenant.delete
{ "tenant_name": "Example Corp" }
```

### 6.3 記録タイミング

操作が**成功した後**に INSERT する。失敗した操作（バリデーションエラー等）は基本的に記録しない。  
例外: `auth.login.failure` は失敗時にのみ記録する。

---

## 7. API

### 7.1 管理者専用 API（ユーザー管理）

すべて `AdminUser` エクストラクタを使用。セッション認証必須（PAT 不可）。

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/admin/users` | 全ユーザー一覧 |
| `POST` | `/v1/admin/users` | ユーザー作成 |
| `PATCH` | `/v1/admin/users/{id}` | `is_admin` / `is_suspended` 変更 |
| `DELETE` | `/v1/admin/users/{id}` | ユーザー強制削除 |
| `POST` | `/v1/admin/users/{id}/password-reset` | パスワードリセットリンク生成 |
| `POST` | `/v1/admin/users/{id}/reset-2fa` | 2FA 強制リセット |
| `DELETE` | `/v1/admin/users/{id}/passkeys/{passkey_id}` | パスキー強制削除 |
| `DELETE` | `/v1/admin/users/{id}/oauth/{provider}` | OAuth 連携強制解除 |

**パスワードリセットリンク生成**:

`POST /v1/admin/users/{id}/password-reset` リクエスト:

```json
{
  "send_to": "support-relay@example.com"
}
```

- `send_to` は省略可。省略時はユーザーの登録メールアドレスに送信する
- 指定した場合はその宛先に送信する（ユーザーがメールアドレスを削除・変更してしまった場合の救済用）
- 生成されたリセットトークンは [パスワードリセット仕様書](/features/auth-password-reset) と同じ TTL（30 分）で有効
- 監査ログに `user.password_reset` + `metadata.reset_email` を記録する

**2FA 強制リセット**:

`POST /v1/admin/users/{id}/reset-2fa`:

- `totp_credentials` レコードを削除し `users.totp_enabled = false` に設定
- リカバリーコードも全削除
- 対象ユーザーの全セッションを無効化（次回ログイン時に再設定を促す）
- 監査ログに `user.2fa.reset` を記録する

### 7.2 管理者専用 API（テナント・閲覧）

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/admin/tenants` | 全テナント一覧 |
| `GET` | `/v1/admin/tenants/{id}` | テナント詳細 |
| `DELETE` | `/v1/admin/tenants/{id}` | テナント強制削除 |
| `GET` | `/v1/admin/tenants/{id}/projects` | テナント配下プロジェクト一覧（読み取り専用） |
| `GET` | `/v1/admin/tenants/{id}/projects/{pid}/tasks` | プロジェクト配下タスク一覧（読み取り専用） |

### 7.3 監査ログ閲覧 API

```text
GET /v1/admin/audit-logs
```

クエリパラメータ:

| パラメータ | 型 | 説明 |
|-----------|-----|------|
| `action` | string | アクション名でフィルタ（前方一致、例: `user.admin`） |
| `resource_type` | string | リソース種別フィルタ |
| `resource_id` | string | リソース ID フィルタ |
| `actor_id` | UUID | 操作者フィルタ |
| `tenant_id` | UUID | テナントフィルタ |
| `from` | ISO8601 | 期間開始 |
| `to` | ISO8601 | 期間終了 |
| `limit` | int | デフォルト 50、最大 200 |
| `cursor` | string | ページネーションカーソル（`created_at` + `id` 複合） |

レスポンス:

```json
{
  "logs": [
    {
      "id": "uuid",
      "actor_id": "uuid",
      "actor_type": "user",
      "action": "user.admin.grant",
      "resource_type": "user",
      "resource_id": "uuid",
      "tenant_id": null,
      "metadata": { "reason": "初期管理者設定" },
      "ip_address": "203.0.113.1",
      "created_at": "2026-05-27T10:00:00Z"
    }
  ],
  "next_cursor": "base64cursor"
}
```

### 7.4 システム設定 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/admin/system/settings` | システム設定取得 |
| `PATCH` | `/v1/admin/system/settings` | システム設定変更 |

設定項目（初期スコープ）:

```json
{
  "user_registration_enabled": true,
  "drive_default_quota_mb": 10240,
  "drive_system_max_quota_mb": 102400
}
```

---

## 8. セキュリティ

| 脅威 | 対策 |
|------|------|
| 管理者権限の横取り | `is_admin` 変更は `AdminUser` エクストラクタ経由のみ。一般ユーザー API では変更不可 |
| 管理者ゼロ状態 | 自己 `is_admin=false` 操作を `403` で拒否 |
| 監査ログ改ざん | `audit_logs` に UPDATE / DELETE を付与しない（DB ロールで制御） |
| ブートストラップの悪用 | `BOOTSTRAP_ADMIN_EMAIL` は起動時のみ参照。本番では環境変数削除を推奨 |
| PAT による管理者操作 | `AdminUser` エクストラクタはセッション認証のみ受理。PAT は `403` |
| パスワードリセット悪用 | リセットリンク生成を監査ログに記録。管理者がパスワードを知ることはできない |

---

## 9. フロントエンド（Phase B）

### 設計原則: 管理者 UI の完全分離

管理者の権限で通常のテナント・プロジェクト・タスク画面を見ても、**一般ユーザーと同じ表示**になる。管理者専用の操作・データは `/admin` 配下のページからのみアクセスできる。

- 通常画面（`/tenants/{id}/...`）に管理者バッジや追加操作ボタンは一切表示しない
- ナビゲーションバーに「管理者」リンクを 1 つ追加するのみ（`is_admin` のときのみ表示）
- テナント・プロジェクト・タスクの管理者用閲覧は `/admin/tenants/{id}/...` の専用ルートで行う

### 管理者ダッシュボード

```text
/admin
```

```text
┌──────────────────────────────────────────────────┐
│ 管理者ダッシュボード                               │
├──────────────────────────────────────────────────┤
│ ユーザー数: 1,234   テナント数: 87   停止中: 3    │
├──────────────────────────────────────────────────┤
│ [ユーザー管理] [テナント管理] [監査ログ] [設定]   │
└──────────────────────────────────────────────────┘
```

### ユーザー管理画面

```text
/admin/users
```

```text
┌────────────────────────────────────────────────────────────────┐
│ ユーザー管理                                    [+ ユーザー作成] │
├──────────────┬──────────────────┬──────────┬───────────────────┤
│ 名前         │ メール           │ 状態     │ 操作              │
├──────────────┼──────────────────┼──────────┼───────────────────┤
│ Alice        │ alice@example.com│ 通常     │ [詳細▼]           │
│ Bob          │ bob@example.com  │ 停止中   │ [詳細▼]           │
└──────────────┴──────────────────┴──────────┴───────────────────┘
```

ユーザー詳細ドロワー（[詳細▼] クリック時）:

```text
┌──────────────────────────────────────┐
│ bob@example.com                      │
│ 状態: 停止中                          │
│                                      │
│ [停止解除]  [パスワードリセット送信]   │
│ [2FA リセット]  [パスキー削除]        │
│ [OAuth 解除]    [強制削除]            │
│                                      │
│ 管理者フラグ: ○ 付与  ● 剥奪         │
└──────────────────────────────────────┘
```

### 監査ログ画面

```text
/admin/audit-logs
```

```text
┌────────────────────────────────────────────────────────────────────┐
│ 監査ログ                            [アクション▼] [期間▼] [🔍検索] │
├──────────────────┬──────────────────┬──────────────────┬───────────┤
│ 日時             │ 操作者           │ アクション        │ 対象      │
├──────────────────┼──────────────────┼──────────────────┼───────────┤
│ 2026-05-27 10:02 │ alice@example.com│ user.admin.grant │ user:bob  │
│ 2026-05-27 09:55 │ system           │ user.admin.grant │ user:alice│
│ 2026-05-27 09:30 │ bob@example.com  │ auth.2fa.enable  │ user:bob  │
└──────────────────┴──────────────────┴──────────────────┴───────────┘
```

### ルートガード

```typescript
// middleware/admin.ts
export default defineNuxtRouteMiddleware(() => {
  const { user } = useAuth()
  if (!user.value?.is_admin) {
    return navigateTo('/')
  }
})
```

`/admin/**` 配下のすべてのページに `middleware: ['auth', 'admin']` を適用する。  
`is_admin` は `/v1/users/me` レスポンスに含め、フロントエンドが保持する。

| コンポーネント | ファイル |
|--------------|---------|
| `AdminLayout` | `layouts/admin.vue` |
| `AdminUserTable` | `components/admin/AdminUserTable.vue` |
| `AdminUserDrawer` | `components/admin/AdminUserDrawer.vue` |
| `AuditLogTable` | `components/admin/AuditLogTable.vue` |
