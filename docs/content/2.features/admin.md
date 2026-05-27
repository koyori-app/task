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

管理者（`is_admin = true`）はすべてのテナント・ユーザーにアクセスでき、通常ユーザーでは不可能な操作（テナント強制削除・ユーザー停止など）を実行できる。  
監査ログはサービスの透明性・セキュリティ調査・コンプライアンス対応のために設計する。

---

## 2. データモデル

### 2.1 `users` テーブルへの追加

```sql
ALTER TABLE users ADD COLUMN is_admin BOOLEAN NOT NULL DEFAULT false;
```

```rust
// entities/users.rs に追加
pub is_admin: bool,
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
    → users WHERE email = $email → is_admin = true に UPDATE
    → audit_logs に action="user.admin.grant", actor_type="system" を INSERT
    → 環境変数をログ出力後、以降は参照しない（毎起動ごとに再実行）
```

> **運用上の注意**: ブートストラップ後は `BOOTSTRAP_ADMIN_EMAIL` 環境変数を削除することを推奨する。毎起動で上書きするため、変数が残っていても害はないが、意図しない権限付与を防ぐために明示的に削除するのがベストプラクティス。

---

## 5. AdminUser エクストラクタ

```rust
/// is_admin = true のユーザー専用エクストラクタ。
pub struct AdminUser {
    pub user_id: Uuid,
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let auth = AuthUser::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

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

管理者専用エンドポイントはすべて `AdminUser` エクストラクタを使用する。非管理者は `403 Forbidden`。

---

## 6. 監査ログ記録対象

### 6.1 アクション一覧

| アクション | resource_type | トリガー |
|-----------|--------------|---------|
| `user.admin.grant` | `user` | 管理者フラグ付与 |
| `user.admin.revoke` | `user` | 管理者フラグ剥奪 |
| `user.suspend` | `user` | ユーザー停止 |
| `user.unsuspend` | `user` | ユーザー停止解除 |
| `user.delete` | `user` | ユーザー強制削除（管理者のみ） |
| `tenant.delete` | `tenant` | テナント強制削除（管理者のみ） |
| `tenant.owner.transfer` | `tenant` | オーナー移譲 |
| `project.member.add` | `project` | プロジェクトメンバー追加 |
| `project.member.remove` | `project` | プロジェクトメンバー除外 |
| `project.member.role.change` | `project` | プロジェクト内ロール変更 |
| `project.delete` | `project` | プロジェクト削除 |
| `auth.login.success` | `user` | ログイン成功 |
| `auth.login.failure` | `user` | ログイン失敗（email から特定できた場合） |
| `auth.2fa.enable` | `user` | 2FA 有効化 |
| `auth.2fa.disable` | `user` | 2FA 無効化 |
| `auth.passkey.add` | `user` | パスキー登録 |
| `auth.passkey.remove` | `user` | パスキー削除 |
| `auth.oauth.connect` | `user` | OAuth 連携 |
| `auth.oauth.disconnect` | `user` | OAuth 連携解除 |
| `auth.pat.create` | `user` | PAT 作成 |
| `auth.pat.delete` | `user` | PAT 削除 |
| `github.integration.connect` | `project` | GitHub 連携設定 |
| `github.integration.disconnect` | `project` | GitHub 連携解除 |

### 6.2 `metadata` フィールドの構造例

```json
// user.admin.grant
{ "granted_by": "<actor_uuid>", "reason": "初期管理者設定" }

// project.member.role.change
{ "before": "Member", "after": "Admin" }

// auth.login.failure
{ "email": "user@example.com", "reason": "invalid_password" }

// tenant.delete
{ "tenant_name": "Example Corp", "forced_by_admin": true }
```

### 6.3 記録タイミング

操作が**成功した後**に INSERT する。失敗した操作（バリデーションエラー等）は基本的に記録しない。  
例外: `auth.login.failure` は失敗時にのみ記録する。

---

## 7. API

### 7.1 管理者専用 API

すべて `AdminUser` エクストラクタを使用。

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/admin/users` | 全ユーザー一覧（ページネーション） |
| `PATCH` | `/v1/admin/users/{id}` | ユーザー属性変更（`is_admin` / `is_suspended`） |
| `DELETE` | `/v1/admin/users/{id}` | ユーザー強制削除 |
| `GET` | `/v1/admin/tenants` | 全テナント一覧（ページネーション） |
| `DELETE` | `/v1/admin/tenants/{id}` | テナント強制削除 |
| `GET` | `/v1/admin/audit-logs` | 監査ログ一覧（後述） |

`PATCH /v1/admin/users/{id}` リクエスト:

```json
{
  "is_admin": true,
  "is_suspended": false
}
```

> **自己操作禁止**: `PATCH /v1/admin/users/{id}` で自分自身の `is_admin` を `false` に変更することは `403` を返す（管理者ゼロ状態を防ぐ）。

### 7.2 監査ログ閲覧 API

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

---

## 8. ユーザー停止（`is_suspended`）

管理者がユーザーを停止できるよう、`users` テーブルに `is_suspended` カラムも追加する。

```sql
ALTER TABLE users ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT false;
```

停止されたユーザーはログイン・API 呼び出しすべてで `403 Forbidden` を返す。  
`AuthUser` エクストラクタで `is_suspended` チェックを追加する。

```rust
// extractors.rs: user_id 取得後に追加
let user = users::Entity::find_by_id(user_id).one(&state.db).await?
    .ok_or(AuthError::Unauthorized)?;
if user.is_suspended {
    return Err(AuthError::Forbidden);
}
```

---

## 9. セキュリティ

| 脅威 | 対策 |
|------|------|
| 管理者権限の横取り | `is_admin` 変更は `AdminUser` エクストラクタ経由のみ。一般ユーザー API では変更不可 |
| 管理者ゼロ状態 | 自己 `is_admin=false` 操作を `403` で拒否 |
| 監査ログ改ざん | `audit_logs` に UPDATE / DELETE を付与しない（DB ロールで制御） |
| ブートストラップの悪用 | `BOOTSTRAP_ADMIN_EMAIL` は起動時のみ参照。本番では環境変数削除を推奨 |
| PAT による管理者操作 | PAT は `is_admin` に関係なく通常ユーザー扱い。管理者操作はセッション必須 |

---

## 10. フロントエンド（Phase B）

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
│ [ユーザー管理] [テナント管理] [監査ログ]            │
└──────────────────────────────────────────────────┘
```

### 監査ログ画面

```text
/admin/audit-logs
```

```text
┌──────────────────────────────────────────────────────────────────┐
│ 監査ログ                                [アクション▼] [期間▼] 🔍 │
├────────────────────────────────────────────────────────────────── │
│ 2026-05-27 10:02  alice@example.com  user.admin.grant   user:bob │
│ 2026-05-27 09:55  system             user.admin.grant   user:alice│
│ 2026-05-27 09:30  bob@example.com    auth.2fa.enable    user:bob │
│ ...                                                               │
└──────────────────────────────────────────────────────────────────┘
```

| コンポーネント | ファイル |
|--------------|---------|
| `AdminLayout` | `layouts/admin.vue` |
| `AdminUserTable` | `components/admin/AdminUserTable.vue` |
| `AuditLogTable` | `components/admin/AuditLogTable.vue` |
