# パーソナルアクセストークン（PAT）認可要件

> **ステータス**: 未実装（別 PR で対応予定）  
> **関連コード**: `src/entities/personal_tokens.rs`, `src/entities/scopes.rs`, `src/extractors.rs`, `src/utils/auth.rs`

## 概要

PAT による API 認証と、テナント・プロジェクトを横断しない権限チェックの要件を定義する。

- **セッション（Cookie）**: 全操作スコープ相当。テナント切り替えは UI から可能。
- **PAT（Bearer）**: 作成時に固定した **1 テナント** と **任意のプロジェクト一覧** の範囲内でのみ有効。操作は `scopes` で制限。

認可は **操作スコープ（何ができるか）** と **リソース束縛（どこでできるか）** の 2 層で行う。

## 確定した方針

### 認証方式ごと

| 認証方式 | 操作スコープ | リソース範囲 |
|----------|-------------|--------------|
| セッション | `require_scope` は常に通過 | メンバーシップに基づきアクセス可能なテナントへアクセス可 |
| PAT | DB の `scopes` を検証 | 作成時の `tenant_id` のみ。プロジェクトは任意指定 |

### PAT のリソース束縛

| 項目 | 方針 |
|------|------|
| テナント | 作成時に **1 件必須**（`tenant_id`）。他テナントの API は 403 |
| プロジェクト | **任意指定**（`allowed_project_ids`）。未指定（`NULL`）= 当該テナント内の全プロジェクト |
| 複数テナント PAT | 採用しない |
| テナント非紐づけ PAT | 採用しない（`/me` 等アカウント API はセッション専用） |
| PAT 管理 API（作成・失効） | **セッション専用**（PAT では不可） |

### エンドポイントでの書き方

**方式 A（確定）**: ハンドラ先頭で認可ヘルパーを呼ぶ。スコープ専用エクストラクタ（`ReadUserAuth` 等）は作らない。

```rust
auth.require_scope(Scope::ReadProject)?;
auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;
```

## 二層モデル

### Layer 1: 操作スコープ

「何ができるか」を `personal_tokens.scopes`（`ScopeList`）で表現する。

| スコープ（案） | 意味 |
|---------------|------|
| `read:project` | プロジェクトの参照 |
| `write:project` | プロジェクトの更新 |
| `read:issue` / `write:issue` | イシュー操作（機能追加時） |
| `admin:tenant` | 当該 PAT の `tenant_id` 内の管理操作（wildcard） |

**廃止・置き換え（案）**

| 旧 | 扱い |
|----|------|
| `read:user` / `write:user` | 削除。`/me` はセッション専用 |
| `admin:all` | `admin:tenant` に置き換え |

スコープ文字列にテナント ID を埋め込まない（例: `read:tenant:uuid` は採用しない）。

### Layer 2: リソース束縛

「どこでできるか」を DB カラムで表現する。

| カラム | 型 | 説明 |
|--------|-----|------|
| `tenant_id` | `UUID` NOT NULL | PAT が有効なテナント（1 件固定） |
| `allowed_project_ids` | `JSON` NULL 可 | 許可プロジェクト ID の配列。`NULL` = テナント内全プロジェクト |

## データモデル変更

### `personal_tokens` テーブル（追加カラム）

```sql
-- migration で追加
tenant_id UUID NOT NULL REFERENCES tenants(id),
allowed_project_ids JSONB NULL  -- ["uuid", ...] または NULL
```

### PAT 作成リクエスト（案）

```json
{
  "name": "CI token",
  "tenant_id": "uuid",
  "project_ids": ["uuid", "uuid"],
  "scopes": ["read:project", "write:issue"],
  "expires_at": "2026-12-31T00:00:00Z"
}
```

作成時の検証:

- リクエスト実行者が `tenant_id` にアクセス可能であること（現状は `tenants.owner_id`、将来は `tenant_members`）
- `project_ids` を指定する場合、すべて当該 `tenant_id` 配下であること

## 認証・認可の実装方針

### `AuthUser` / `AuthMethod`

```rust
pub enum AuthMethod {
    Session,
    PersonalToken {
        token_id: Uuid,
        tenant_id: Uuid,
        allowed_project_ids: Option<Vec<Uuid>>,
        scopes: ScopeList,
    },
}

pub struct AuthUser {
    pub user_id: Uuid,
    pub method: AuthMethod,
}
```

### 認証フロー（`FromRequestParts`）

1. `Authorization: Bearer <token>` があれば PAT 認証
2. なければ Cookie セッションから `user_id` 取得
3. どちらもなければ **401**

PAT 認証（`authenticate_personal_token`）:

1. Bearer トークンを HMAC ハッシュ化（`PERSONAL_TOKEN_SECRET`、`utils/auth.rs`）
2. `personal_tokens` を `token_hash` で 1 SELECT
3. `verify_personal_token`、失効・期限切れチェック
4. `AuthMethod::PersonalToken { ... }` を構築（**以降の認可はメモリのみ**）
5. （任意）`last_used_at` は fire-and-forget

### 認可ヘルパー

| メソッド | セッション | PAT |
|----------|-----------|-----|
| `require_scope` | 常に OK | `scopes` を検証。不足なら 403 |
| `ensure_tenant_access` | メンバーシップ 1 SELECT（推奨） | `token.tenant_id == path.tenant_id`（メモリ） |
| プロジェクト検証（同上に含める） | 同一クエリで `project.tenant_id` 確認可 | `allowed_project_ids` が `None` なら OK、否则は包含チェック（メモリ） |

### HTTP ステータス

| 状況 | ステータス |
|------|-----------|
| 未認証・無効 PAT・失効・期限切れ | 401 |
| 認証済みだがスコープ / テナント / プロジェクト不一致 | 403 |

## API パス規約

テナント配下のリソースは path にテナント（・プロジェクト）を含める。

```
GET /v1/tenants/{tenant_id}/projects/{project_id}/...
```

path の ID と PAT の `tenant_id` / `allowed_project_ids` を突き合わせる。

アカウント API（例: `GET /v1/auth/me`）は PAT 非対応（セッションのみ）。

## DB アクセス回数

| 認証 | 認可まわりの DB（目安） |
|------|------------------------|
| PAT | 認証時 `personal_tokens` **1 SELECT**。`require_scope` / テナント・プロジェクトチェックは **追加クエリなし** |
| セッション | Redis セッション + テナント所属 **0〜1 SELECT** |

### 避けること

- `require_*` ごとに DB を再クエリする
- ハンドラごとに Bearer パース + トークン lookup を重複する

### 将来の最適化（必要時）

- セッションのテナントメンバーシップを Redis キャッシュ（TTL）
- `last_used_at` 更新を非同期化

## 変更対象ファイル（実装 PR 用チェックリスト）

- [ ] migration: `tenant_id`, `allowed_project_ids`
- [ ] `src/entities/personal_tokens.rs`
- [ ] `src/entities/scopes.rs`（列挙整理 + `ScopeList::has_scope`）
- [ ] `src/extractors.rs`（`AuthUser`, `SessionAuth`, PAT 認証）
- [ ] `src/services/authz.rs`（新規: メンバーシップ検証）
- [ ] `src/handlers/personal_tokens.rs`（作成・失効）
- [ ] テナント配下 handlers（認可 3 段）
- [ ] OpenAPI: `bearerAuth`、PAT 作成スキーマ

## テスト要件

- `ScopeList::has_scope`: `admin:tenant`、不足スコープ
- `require_scope`: Session は常に OK、PAT は不足で 403
- PAT が別テナントの path を叩く → 403
- `allowed_project_ids` 外の project → 403、`NULL` ならテナント内任意 project → OK
- 失効 / 期限切れ / ハッシュ不一致 → 401
- PAT 作成: 他人の `tenant_id` → 403

## 採用しない方針

- セッションにスコープを持たせる
- 複数テナント / テナント非紐づけ PAT
- スコープ専用エクストラクタ（方式 B）
- Route Layer での一括スコープ管理
- スコープ文字列へのリソース ID 埋め込み
- `scopes` だけでテナント境界を守る（必ず `tenant_id` カラムと併用）

## 参考

- GitHub: PAT scopes + fine-grained PAT の repository 指定
- GitLab: project access token
