---
title: タスク機能仕様
description: タスク管理システムのコア機能設計仕様書
icon: lucide:check-square
---

# タスク機能仕様書

> ステータス: **Draft**
> 作成日: 2026-05-27

---

## 1. 概要

プロジェクト横断で利用できるタスク管理のコア機能仕様。  
担当者・締切・工数追跡・ガントチャートを軸に、チームの作業可視化と進捗管理を実現する。

ストレージ統合は既存の [Drive機能](/features/drive) を利用する。

---

## 2. スコープ

### Phase A（MVP）— バックエンド

| 機能 | 内容 |
|------|------|
| タスク CRUD | 作成・取得・更新・削除 |
| 担当者管理 | Primary / Secondary の複数アサイン |
| 締切管理 | 仮締め（Soft）/ Deadline（Hard）の 2 種 |
| 優先順位 | 6 段階（炎上 〜 雑魚） |
| 作業時間追跡 | タイマー起動 / 停止・手動ログ入力 |
| 親子・依存関係 | サブタスク階層 / blocks / blocked_by |
| マイルストーン | プロジェクト単位で作成・タスク紐付け |
| ラベル | プロジェクト単位で作成・エクスポート / インポート |
| ファイル添付 | Drive と統合 |
| Webhook | created / updated（差分のみ）/ deleted |

### Phase B — フロントエンド（Phase A 完全完了後に着手）

| 機能 | 内容 |
|------|------|
| タスク一覧 / 詳細 | ボード・リスト・テーブル表示 |
| カレンダービュー | 月 / 週 / 日の 3 モード |
| ガントチャート | イナズマ線付き（計画 vs 実績） |
| 依存関係グラフ | Relations タブ |
| ラベル管理 UI | エクスポート / インポート |
| ファイル添付プレビュー | 画像 / 動画 |
| Webhook 設定画面 | |

---

## 3. データモデル

### 3.1 `tasks` テーブル

```rust
// entities/tasks.rs
pub struct Model {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub description: Option<String>,       // Markdown テキスト
    pub status: TaskStatus,                // enum
    pub priority: TaskPriority,            // enum
    pub progress_pct: i16,                 // 0–100
    pub parent_task_id: Option<Uuid>,      // 自己参照（ルートタスクは None）
    pub milestone_id: Option<Uuid>,
    pub soft_deadline: Option<DateTimeUtc>,
    pub hard_deadline: Option<DateTimeUtc>,
    pub estimated_minutes: Option<i32>,    // 見積もり工数（分単位で保持）
    pub created_by: Uuid,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,   // ソフトデリート
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `title` | VARCHAR(255) | NOT NULL | |
| `description` | TEXT | NULLABLE | Markdown |
| `status` | VARCHAR | NOT NULL DEFAULT 'open' | `open` / `in_progress` / `blocked` / `done` / `overdue` |
| `priority` | VARCHAR | NOT NULL DEFAULT 'medium' | `critical_fire` / `critical` / `high` / `medium` / `low` / `trivial` |
| `progress_pct` | SMALLINT | NOT NULL DEFAULT 0 CHECK (0–100) | |
| `parent_task_id` | UUID | NULLABLE, FK→tasks SET NULL | 自己参照 |
| `milestone_id` | UUID | NULLABLE, FK→milestones SET NULL | |
| `soft_deadline` | TIMESTAMPTZ | NULLABLE | |
| `hard_deadline` | TIMESTAMPTZ | NULLABLE | |
| `estimated_minutes` | INT | NULLABLE | |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `deleted_at` | TIMESTAMPTZ | NULLABLE | ソフトデリート |

> **優先順位 enum の DB 値マッピング**:
> `炎上` → `critical_fire`, `Critical` → `critical`, `High` → `high`,
> `Medium` → `medium`, `Low` → `low`, `雑魚` → `trivial`

> **ソフトデリート**: `deleted_at IS NOT NULL` のタスクは全クエリで自動除外する。
> 物理削除は管理者 API のみ提供（Phase A 対象外）。

### 3.2 `task_assignees` テーブル

```rust
// entities/task_assignees.rs
pub struct Model {
    pub id: Uuid,
    pub task_id: Uuid,
    pub user_id: Uuid,
    pub role: AssigneeRole,   // primary | secondary
    pub assigned_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `role` | VARCHAR | NOT NULL DEFAULT 'secondary' | `primary` / `secondary` |
| `assigned_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(task_id, user_id) | 同一ユーザーの重複防止 |

### 3.3 `task_relations` テーブル

```rust
// entities/task_relations.rs
pub struct Model {
    pub id: Uuid,
    pub blocker_task_id: Uuid,   // blocks 側
    pub blocked_task_id: Uuid,   // blocked_by 側
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `blocker_task_id` | UUID | NOT NULL, FK→tasks CASCADE | ブロックする側 |
| `blocked_task_id` | UUID | NOT NULL, FK→tasks CASCADE | ブロックされる側 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(blocker_task_id, blocked_task_id) | 重複防止 |
| — | — | CHECK(blocker_task_id <> blocked_task_id) | 自己参照防止 |

> **循環依存の検出**: タスク A → B のリレーション追加時、B から A への到達パスを探索し、循環が検出された場合は `409 Conflict` を返す。

### 3.4 `time_logs` テーブル

```rust
// entities/time_logs.rs
pub struct Model {
    pub id: Uuid,
    pub task_id: Uuid,
    pub user_id: Uuid,
    pub logged_minutes: i32,         // 実績工数（分単位）
    pub logged_at: NaiveDate,        // 作業日
    pub note: Option<String>,
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `logged_minutes` | INT | NOT NULL CHECK (> 0) | |
| `logged_at` | DATE | NOT NULL | |
| `note` | TEXT | NULLABLE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> **タイマー**: アクティブなタイマーはインメモリ（Redis or DB）で管理。Stop 時に `time_logs` へ書き込む。同一ユーザーが同じタスクで複数タイマーを同時起動するとエラー（`409`）。

### 3.5 `milestones` テーブル

```rust
// entities/milestones.rs
pub struct Model {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub due_date: NaiveDate,
    pub created_by: Uuid,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `name` | VARCHAR(255) | NOT NULL | |
| `description` | TEXT | NULLABLE | |
| `due_date` | DATE | NOT NULL | |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

### 3.6 `labels` テーブル（既存 + `project_id` 追加）

既存の `labels` テーブルはグローバルスコープになっているため、`project_id` カラムを追加してプロジェクト単位で管理する。

```rust
// entities/labels.rs（更新後）
pub struct Model {
    pub id: Uuid,
    pub project_id: Uuid,          // 追加
    pub name: String,
    pub description: String,
    pub color: String,             // hex (#e11d48)
    pub icon_url: Option<String>,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | **新規追加** |
| — | — | UNIQUE(project_id, name) | プロジェクト内で名称重複禁止 |

### 3.7 `task_labels` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `label_id` | UUID | NOT NULL, FK→labels CASCADE | |
| — | — | PRIMARY KEY(task_id, label_id) | |

> **クロスプロジェクト制約**: `task.project_id = label.project_id` をアプリ層で検証。別プロジェクトのラベルを付与しようとすると `400 Bad Request`。

### 3.8 `task_attachments` テーブル

Drive の `drive_files` をタスクへ紐付ける中間テーブル。

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `drive_file_id` | UUID | NOT NULL, FK→drive_files CASCADE | |
| `attached_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | PRIMARY KEY(task_id, drive_file_id) | |

### 3.9 `webhooks` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `url` | VARCHAR(2048) | NOT NULL | 送信先 URL |
| `secret` | VARCHAR | NOT NULL | HMAC-SHA256 署名用シークレット |
| `events` | VARCHAR[] | NOT NULL | 有効にするイベント一覧 |
| `is_active` | BOOLEAN | NOT NULL DEFAULT true | 連続失敗時に自動 false |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

### 3.10 `webhook_deliveries` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `webhook_id` | UUID | NOT NULL, FK→webhooks CASCADE | |
| `event` | VARCHAR | NOT NULL | イベント名 |
| `payload` | JSONB | NOT NULL | 送信ペイロード |
| `status_code` | INT | NULLABLE | HTTP レスポンスコード |
| `attempt` | SMALLINT | NOT NULL DEFAULT 1 | リトライ回数（最大 5） |
| `delivered_at` | TIMESTAMPTZ | NULLABLE | 成功時のみ |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

---

## 4. アクセス制御

### 4.1 アクセスルール

| 操作 | 権限 |
|------|------|
| タスク閲覧 | プロジェクトメンバー **または** テナントオーナー |
| タスク作成 / 更新 / 削除 | プロジェクトメンバー **または** テナントオーナー |
| タスク削除（物理） | テナントオーナーのみ（Phase A 対象外） |
| ラベル管理 | プロジェクトメンバー **または** テナントオーナー |
| Webhook 設定 | テナントオーナーのみ |

### 4.2 スコープ

既存の Scope enum に以下を追加する:

```rust
pub enum Scope {
    // 既存
    ReadProject,
    WriteProject,
    // 追加
    ReadTask,
    WriteTask,
    ReadMilestone,
    WriteMilestone,
    ManageWebhook,
}
```

---

## 5. マイグレーション

マイグレーションは既存パターン（raw SQL + `sea_orm_migration`）に従う。

```rust
// migration/src/m20260527_000000_create_tasks.rs

async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
    let sql = r#"
        CREATE TABLE milestones (
            id UUID PRIMARY KEY,
            project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            name VARCHAR(255) NOT NULL,
            description TEXT,
            due_date DATE NOT NULL,
            created_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE tasks (
            id UUID PRIMARY KEY,
            project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            title VARCHAR(255) NOT NULL,
            description TEXT,
            status VARCHAR NOT NULL DEFAULT 'open',
            priority VARCHAR NOT NULL DEFAULT 'medium',
            progress_pct SMALLINT NOT NULL DEFAULT 0 CHECK (progress_pct BETWEEN 0 AND 100),
            parent_task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
            milestone_id UUID REFERENCES milestones(id) ON DELETE SET NULL,
            soft_deadline TIMESTAMPTZ,
            hard_deadline TIMESTAMPTZ,
            estimated_minutes INT CHECK (estimated_minutes > 0),
            created_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            deleted_at TIMESTAMPTZ,
            CONSTRAINT soft_before_hard CHECK (
                soft_deadline IS NULL OR hard_deadline IS NULL OR soft_deadline <= hard_deadline
            )
        );

        CREATE TABLE task_assignees (
            id UUID PRIMARY KEY,
            task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            role VARCHAR NOT NULL DEFAULT 'secondary',
            assigned_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (task_id, user_id)
        );

        CREATE TABLE task_relations (
            id UUID PRIMARY KEY,
            blocker_task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            blocked_task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (blocker_task_id, blocked_task_id),
            CHECK (blocker_task_id <> blocked_task_id)
        );

        CREATE TABLE time_logs (
            id UUID PRIMARY KEY,
            task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            logged_minutes INT NOT NULL CHECK (logged_minutes > 0),
            logged_at DATE NOT NULL,
            note TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE task_labels (
            task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            label_id UUID NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
            PRIMARY KEY (task_id, label_id)
        );

        CREATE TABLE task_attachments (
            task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
            drive_file_id UUID NOT NULL REFERENCES drive_files(id) ON DELETE CASCADE,
            attached_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (task_id, drive_file_id)
        );

        CREATE TABLE webhooks (
            id UUID PRIMARY KEY,
            project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            url VARCHAR(2048) NOT NULL,
            secret VARCHAR NOT NULL,
            events VARCHAR[] NOT NULL,
            is_active BOOLEAN NOT NULL DEFAULT true,
            created_by UUID NOT NULL REFERENCES users(id),
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE webhook_deliveries (
            id UUID PRIMARY KEY,
            webhook_id UUID NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
            event VARCHAR NOT NULL,
            payload JSONB NOT NULL,
            status_code INT,
            attempt SMALLINT NOT NULL DEFAULT 1,
            delivered_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        -- labels テーブルへ project_id を追加
        ALTER TABLE labels ADD COLUMN project_id UUID REFERENCES projects(id) ON DELETE CASCADE;

        CREATE INDEX idx_tasks_project_id ON tasks(project_id) WHERE deleted_at IS NULL;
        CREATE INDEX idx_tasks_parent ON tasks(parent_task_id) WHERE parent_task_id IS NOT NULL;
        CREATE INDEX idx_tasks_milestone ON tasks(milestone_id) WHERE milestone_id IS NOT NULL;
        CREATE INDEX idx_task_assignees_user ON task_assignees(user_id);
        CREATE INDEX idx_time_logs_task ON time_logs(task_id);
        CREATE INDEX idx_time_logs_user_date ON time_logs(user_id, logged_at);
    "#;
    // ...
}
```

---

## 6. API 設計

全エンドポイントはセッション認証または PAT 認証が必須（既存の `AuthUser` extractor を流用）。  
URL の基底パスは `/v1/tenants/{tenant_id}/projects/{project_id}` 。

### 6.1 タスク API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/tasks` | ReadTask | タスク一覧 |
| `POST` | `/tasks` | WriteTask | タスク作成 |
| `GET` | `/tasks/{task_id}` | ReadTask | タスク取得 |
| `PUT` | `/tasks/{task_id}` | WriteTask | タスク更新 |
| `DELETE` | `/tasks/{task_id}` | WriteTask | タスク削除（ソフト） |

#### GET `/tasks` クエリパラメータ

| パラメータ | 型 | デフォルト | 説明 |
|-----------|-----|---------|------|
| `status` | string? | — | フィルター: `open` / `in_progress` / `done` / ... |
| `priority` | string? | — | フィルター: `high` / `medium` / ... |
| `assignee_id` | UUID? | — | 担当者でフィルター |
| `milestone_id` | UUID? | — | マイルストーンでフィルター |
| `label_id` | UUID? | — | ラベルでフィルター |
| `parent_task_id` | UUID? | — | 子タスク一覧に使用 |
| `limit` | u32 | 50 | 最大 200 |
| `offset` | u32 | 0 | |

レスポンス:

```json
{
  "tasks": [
    {
      "id": "uuid",
      "title": "ログイン機能を実装する",
      "status": "in_progress",
      "priority": "high",
      "progress_pct": 40,
      "assignees": [
        { "user_id": "uuid", "role": "primary" }
      ],
      "soft_deadline": "2026-06-01T00:00:00Z",
      "hard_deadline": "2026-06-10T00:00:00Z",
      "estimated_minutes": 120,
      "labels": [{ "id": "uuid", "name": "feature", "color": "#2563eb" }],
      "milestone_id": "uuid",
      "parent_task_id": null,
      "subtask_count": 3,
      "created_at": "2026-05-27T10:00:00Z",
      "updated_at": "2026-05-27T12:00:00Z"
    }
  ],
  "total": 42
}
```

#### POST `/tasks` リクエスト

```json
{
  "title": "ログイン機能を実装する",
  "description": "## 概要\nメール+パスワード認証を実装する",
  "priority": "high",
  "soft_deadline": "2026-06-01T00:00:00Z",
  "hard_deadline": "2026-06-10T00:00:00Z",
  "estimated_minutes": 120,
  "assignees": [
    { "user_id": "uuid", "role": "primary" }
  ],
  "label_ids": ["uuid"],
  "milestone_id": "uuid",
  "parent_task_id": null
}
```

#### PUT `/tasks/{task_id}` リクエスト

すべてのフィールドは optional（PATCH 的な動作）。`null` を渡すとフィールドをクリア。

```json
{
  "title": "ログイン機能を実装する（OAuth対応含む）",
  "priority": "critical",
  "progress_pct": 60,
  "hard_deadline": null
}
```

### 6.2 担当者 API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/tasks/{task_id}/assignees` | ReadTask | 担当者一覧 |
| `POST` | `/tasks/{task_id}/assignees` | WriteTask | 担当者追加 |
| `PUT` | `/tasks/{task_id}/assignees/{user_id}` | WriteTask | ロール変更 |
| `DELETE` | `/tasks/{task_id}/assignees/{user_id}` | WriteTask | 担当者削除 |

`POST` リクエスト:

```json
{ "user_id": "uuid", "role": "secondary" }
```

### 6.3 作業時間追跡 API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/tasks/{task_id}/time-logs` | ReadTask | ログ一覧 |
| `POST` | `/tasks/{task_id}/time-logs` | WriteTask | 手動ログ追加 |
| `DELETE` | `/tasks/{task_id}/time-logs/{log_id}` | WriteTask | ログ削除 |
| `POST` | `/tasks/{task_id}/timer/start` | WriteTask | タイマー開始 |
| `POST` | `/tasks/{task_id}/timer/stop` | WriteTask | タイマー停止→ログ生成 |

手動ログ `POST` リクエスト:

```json
{
  "logged_minutes": 90,
  "logged_at": "2026-05-27",
  "note": "設計レビュー対応"
}
```

タイマー停止レスポンス（生成されたログを返す）:

```json
{
  "id": "uuid",
  "logged_minutes": 47,
  "logged_at": "2026-05-27",
  "note": null
}
```

工数サマリー取得:

```
GET /tasks/{task_id}/time-logs/summary
```

```json
{
  "estimated_minutes": 120,
  "actual_minutes": 90,
  "remaining_minutes": 30,
  "is_over": false,
  "by_user": [
    { "user_id": "uuid", "minutes": 90 }
  ]
}
```

### 6.4 親子・依存関係 API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/tasks/{task_id}/relations` | ReadTask | 関係一覧（blocks / blocked_by / subtasks） |
| `POST` | `/tasks/{task_id}/relations` | WriteTask | 関係追加 |
| `DELETE` | `/tasks/{task_id}/relations/{relation_id}` | WriteTask | 関係削除 |

#### ブロッキング関係の方向性

ブロッキング関係は **どちらのタスクからでも設定できる**。`type` フィールドで方向を指定する。

| `type` | 意味 | 作成されるレコード |
|--------|------|-------------------|
| `"blocks"` | 現在のタスクが `target` をブロックする | `blocker=現タスク`, `blocked=target` |
| `"blocked_by"` | 現在のタスクは `target` にブロックされている | `blocker=target`, `blocked=現タスク` |

**例: タスク A がタスク B をブロックしている場合**

タスク A の画面から設定する場合（「私が B をブロックしている」）:

```json
POST /tasks/{task_a_id}/relations
{ "type": "blocks", "target_task_id": "{task_b_id}" }
```

タスク B の画面から設定する場合（「私は A にブロックされている」）:

```json
POST /tasks/{task_b_id}/relations
{ "type": "blocked_by", "target_task_id": "{task_a_id}" }
```

どちらも `task_relations` テーブルに同一レコード（`blocker=A`, `blocked=B`）が1件だけ作成される。重複登録は `409 Conflict`。

循環依存（A→B→C→A）が発生する場合も `409 Conflict`。

#### `GET` レスポンス

タスク A から見た場合:

```json
{
  "subtasks": [{ "id": "uuid", "title": "..." }],
  "blocks": [
    { "id": "{task_b_id}", "title": "タスク B", "status": "blocked", "relation_id": "uuid" }
  ],
  "blocked_by": [
    { "id": "{task_c_id}", "title": "タスク C", "status": "done", "relation_id": "uuid" }
  ]
}
```

### 6.5 ファイル添付 API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/tasks/{task_id}/attachments` | ReadTask | 添付ファイル一覧 |
| `POST` | `/tasks/{task_id}/attachments` | WriteTask | ファイル添付（Drive ファイルを紐付け） |
| `DELETE` | `/tasks/{task_id}/attachments/{file_id}` | WriteTask | 添付解除（Drive ファイル本体は残る） |

`POST` リクエスト（既存の Drive ファイルを紐付ける）:

```json
{ "drive_file_id": "uuid" }
```

アップロードと同時に添付する場合は Drive API でアップロードして `drive_file_id` を取得してから本 API を叩く。

### 6.6 マイルストーン API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/milestones` | ReadMilestone | 一覧 |
| `POST` | `/milestones` | WriteMilestone | 作成 |
| `GET` | `/milestones/{milestone_id}` | ReadMilestone | 取得（関連タスク一覧・完了率を含む） |
| `PUT` | `/milestones/{milestone_id}` | WriteMilestone | 更新 |
| `DELETE` | `/milestones/{milestone_id}` | WriteMilestone | 削除（タスクの `milestone_id` は NULL にリセット） |

`GET /milestones/{milestone_id}` レスポンス:

```json
{
  "id": "uuid",
  "name": "v1.0 リリース",
  "description": "MVP 機能の完成",
  "due_date": "2026-07-01",
  "progress_pct": 33,
  "task_counts": { "total": 12, "done": 4 },
  "created_at": "2026-05-27T10:00:00Z"
}
```

### 6.7 ラベル API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/labels` | ReadTask | プロジェクトのラベル一覧 |
| `POST` | `/labels` | WriteTask | ラベル作成 |
| `PUT` | `/labels/{label_id}` | WriteTask | ラベル更新 |
| `DELETE` | `/labels/{label_id}` | WriteTask | ラベル削除（task_labels も CASCADE 削除） |
| `GET` | `/labels/export` | ReadTask | ラベル一覧を JSON でエクスポート |
| `POST` | `/labels/import` | WriteTask | JSON からラベルをインポート |

`GET /labels/export` レスポンス（`Content-Type: application/json`, `Content-Disposition: attachment`）:

```json
{
  "version": 1,
  "labels": [
    { "name": "bug", "color": "#e11d48", "description": "不具合報告" },
    { "name": "feature", "color": "#2563eb", "description": "新機能" }
  ]
}
```

`POST /labels/import` リクエスト（上記 JSON をそのまま送信）:

| フィールド | 型 | 説明 |
|-----------|-----|------|
| `on_conflict` | `"skip"` \| `"overwrite"` | 同名ラベルの扱い（デフォルト `"skip"`） |

レスポンス:

```json
{
  "created": 3,
  "skipped": 1,
  "overwritten": 0
}
```

### 6.8 Webhook API

| メソッド | パス | スコープ | 説明 |
|---------|------|---------|------|
| `GET` | `/webhooks` | ManageWebhook | Webhook 一覧 |
| `POST` | `/webhooks` | ManageWebhook | Webhook 作成 |
| `PUT` | `/webhooks/{webhook_id}` | ManageWebhook | Webhook 更新 |
| `DELETE` | `/webhooks/{webhook_id}` | ManageWebhook | Webhook 削除 |
| `GET` | `/webhooks/{webhook_id}/deliveries` | ManageWebhook | 配信履歴 |

`POST /webhooks` リクエスト:

```json
{
  "url": "https://example.com/webhook",
  "secret": "my-secret-token",
  "events": ["task.created", "task.updated", "task.deleted"]
}
```

---

## 7. Webhook ペイロード仕様

リクエストヘッダー:

```
X-Task-Event: task.created
X-Task-Signature: sha256=<HMAC-SHA256-hex>
Content-Type: application/json
```

署名検証（受信側):

```
HMAC-SHA256(secret, raw_request_body) == X-Task-Signature の値
```

### task.created

```json
{
  "event": "task.created",
  "timestamp": "2026-05-27T10:00:00Z",
  "project_id": "uuid",
  "task": {
    "id": "uuid",
    "title": "ログイン機能を実装する",
    "priority": "high",
    "status": "open",
    "assignees": [{ "user_id": "uuid", "role": "primary" }],
    "soft_deadline": "2026-06-01T00:00:00Z",
    "hard_deadline": "2026-06-10T00:00:00Z",
    "label_ids": ["uuid"],
    "milestone_id": null,
    "parent_task_id": null,
    "created_by": "uuid"
  }
}
```

### task.updated

変更されたフィールドのみ差分で送信する:

```json
{
  "event": "task.updated",
  "timestamp": "2026-05-27T11:30:00Z",
  "project_id": "uuid",
  "task_id": "uuid",
  "updated_by": "uuid",
  "changes": [
    { "field": "priority", "old_value": "medium", "new_value": "high" },
    { "field": "status",   "old_value": "open",   "new_value": "in_progress" }
  ]
}
```

### task.deleted

```json
{
  "event": "task.deleted",
  "timestamp": "2026-05-27T12:00:00Z",
  "project_id": "uuid",
  "task_id": "uuid",
  "deleted_by": "uuid"
}
```

### リトライ

- `2xx` 以外のレスポンス: 指数バックオフで最大 5 回リトライ
  - 1 回目: 即時, 2 回目: 30s, 3 回目: 5m, 4 回目: 30m, 5 回目: 2h
- 5 回連続失敗: `webhooks.is_active = false` に設定し、テナントオーナーへメール通知
- 配信履歴は `webhook_deliveries` テーブルに保存（90 日後に自動パージ）

---

## 8. フロントエンド UI 設計

**Phase B で実装。Phase A（バックエンド）完全完了後に着手すること。**

### 8.1 ページ構成（vike + Vue）

```
/tenants/{tenant_id}/projects/{project_id}/tasks          # タスク一覧（ボード / リスト）
/tenants/{tenant_id}/projects/{project_id}/tasks/{id}     # タスク詳細
/tenants/{tenant_id}/projects/{project_id}/calendar       # カレンダービュー
/tenants/{tenant_id}/projects/{project_id}/gantt          # ガントチャート
/tenants/{tenant_id}/projects/{project_id}/milestones     # マイルストーン一覧
/tenants/{tenant_id}/projects/{project_id}/labels         # ラベル管理
/tenants/{tenant_id}/projects/{project_id}/settings/webhooks  # Webhook 設定
```

### 8.2 タスク詳細レイアウト

```
┌──────────────────────────────────────────────────────────────────┐
│ ← プロジェクト名  /  タスクタイトル                  [完了にする] │
├────────────────────────────────────┬─────────────────────────────┤
│                                    │ 担当者                       │
│  # ログイン機能を実装する           │   🧑 田中（Primary）         │
│                                    │   🧑 鈴木（Secondary）       │
│  ## 概要                           │                             │
│  メール+パスワード認証を実装する    │ 優先度  🔴 High              │
│                                    │ ステータス  In Progress      │
│  - [ ] JWT 発行                    │ 進捗率  [████░░░░░] 40%      │
│  - [x] DB スキーマ                 │                             │
│                                    │ 仮締め  2026-06-01          │
│  ─────────────────────────────     │ Deadline  2026-06-10        │
│                                    │                             │
│  📎 添付ファイル                   │ 見積  2h                    │
│  [🖼 screen.png] [📄 spec.pdf]     │ 実績  1h30m  ▶ タイマー     │
│                                    │                             │
│  💬 コメント（Phase 2）            │ ラベル  [feature] [auth]    │
│                                    │ マイルストーン  v1.0         │
│  ─────────────────────────────     │                             │
│  🔗 Relations                      │ 親タスク  #42 認証基盤       │
│  blocks: #55, #56                  │                             │
│  blocked by: #38                   │ ─────────────────────────── │
│                                    │ サブタスク                   │
│  [グラフで表示]                    │   □ #60 JWT実装              │
│                                    │   □ #61 テスト              │
└────────────────────────────────────┴─────────────────────────────┘
```

### 8.3 カレンダービュー

```
┌──────────────────────────────────────────────────────────────────┐
│ [月] [週] [日]    ← 2026年6月 →    担当者: [全員 ▼]            │
├────────┬────────┬────────┬────────┬────────┬────────┬────────────┤
│  月    │  火    │  水    │  木    │  金    │  土    │  日        │
├────────┼────────┼────────┼────────┼────────┼────────┼────────────┤
│        │        │  1     │  2     │  3     │  4     │  5         │
│        │        │ ░░ JWT │▓▓▓▓▓▓▓│▓▓▓▓▓   │        │            │
│        │        │        │←─DB──→│        │        │            │
├────────┼────────┼────────┼────────┼────────┼────────┼────────────┤
│  8     │  9     │ 10     │ 11     │ 12     │ 13     │ 14         │
│        │        │        │        │ ★v1.0  │        │            │
└────────┴────────┴────────┴────────┴────────┴────────┴────────────┘

░░ = 仮締めタスク（破線）  ▓▓ = Deadline タスク（実線）  ★ = マイルストーン
```

### 8.4 ガントチャート + イナズマ線

```
タスク名           │ 5/27 │ 6/01 │ 6/05 │ 6/10 │
───────────────────┼──────┼──────┼──────┼──────┤
DB スキーマ        │██████│      │      │      │
JWT 実装           │  ░░░░│█████ │      │      │
テスト             │      │      │░░░░░ │▓▓▓▓▓ │
───────────────────┼──────┼──────┼──────┼──────┤
♦ v1.0 リリース   │      │      │      │  ◆   │
                   │      │  ↑   │      │      │
                   │      │TODAY │      │      │

イナズマ線（赤 = 遅延）:
  DB スキーマ: 計画100% → 実績100%  → 右に折れる（青）
  JWT 実装:    計画 60% → 実績 40%  → 左に折れる（赤）
```

### 8.5 コンポーネント構成

| コンポーネント | ファイル | 説明 |
|--------------|---------|------|
| `TaskListPage` | `pages/tasks/+Page.vue` | タスク一覧（ボード / リスト切替） |
| `TaskDetailPage` | `pages/tasks/[id]/+Page.vue` | タスク詳細 |
| `TaskCard` | `components/tasks/TaskCard.vue` | ボード用カード |
| `TaskDetailPanel` | `components/tasks/TaskDetailPanel.vue` | 詳細右ペイン |
| `AssigneeSelector` | `components/tasks/AssigneeSelector.vue` | 担当者選択 |
| `TimerWidget` | `components/tasks/TimerWidget.vue` | タイマー UI |
| `RelationsGraph` | `components/tasks/RelationsGraph.vue` | 依存関係グラフ（D3.js） |
| `CalendarPage` | `pages/calendar/+Page.vue` | カレンダービュー |
| `GanttPage` | `pages/gantt/+Page.vue` | ガントチャート |
| `GanttBar` | `components/gantt/GanttBar.vue` | バー描画 |
| `ProgressLine` | `components/gantt/ProgressLine.vue` | イナズマ線描画（Canvas） |
| `MilestoneDiamond` | `components/gantt/MilestoneDiamond.vue` | マイルストーン ◆ |
| `LabelManager` | `pages/labels/+Page.vue` | ラベル管理（エクスポート / インポート UI） |
| `WebhookSettings` | `pages/settings/webhooks/+Page.vue` | Webhook 設定 |

---

## 9. セキュリティ

| 脅威 | 対策 |
|------|------|
| 他プロジェクトのタスクへのアクセス | 全エンドポイントで `project_id` の所属チェック |
| 非メンバーによるタスク閲覧 | `project_members` テーブルでメンバー確認、非メンバーは `403` |
| Webhook シークレットの漏洩 | DB には平文保存、GET レスポンスでは `***` にマスク |
| 循環依存によるグラフ探索無限ループ | `task_relations` 追加時にサーバー側で BFS/DFS で循環検出 |
| XSS（description の Markdown） | フロントエンドは `DOMPurify` でサニタイズしてから描画 |
| タイマーの多重起動 | `timer_sessions` レコードに `UNIQUE(task_id, user_id)` で DB レベル保護 |

---

## 10. 実装方針

**バックエンド完全完了後にフロントエンドへ移行する。**

### Phase A — バックエンド

| # | 内容 | 完了条件 |
|---|------|---------|
| 1 | マイグレーション（全テーブル作成、`labels` に `project_id` 追加） | `sea_orm_migration run` が正常完了 |
| 2 | エンティティ定義（`tasks`, `task_assignees`, `task_relations`, `time_logs`, `milestones`, `task_labels`, `task_attachments`, `webhooks`） | コンパイル通過 |
| 3 | タスク CRUD API | Scalar で動作確認 |
| 4 | 担当者 API | 同上 |
| 5 | 作業時間追跡 API（手動ログ + タイマー） | 同上 |
| 6 | 親子・依存関係 API（循環検出含む） | 同上 |
| 7 | ファイル添付 API（Drive 統合） | 同上 |
| 8 | マイルストーン API | 同上 |
| 9 | ラベル API（プロジェクトスコープ + エクスポート / インポート） | 同上 |
| 10 | Webhook 送信基盤（非同期 Job + リトライ + 自動無効化） | 同上 |
| 11 | `pnpm openapi` でクライアント再生成、型エラーなし | — |

### Phase B — フロントエンド（Phase A 完了後に着手）

| # | 内容 |
|---|------|
| 12 | タスク一覧（ボード / リスト表示） |
| 13 | タスク詳細画面（右ペイン構成） |
| 14 | 担当者セレクタ + タイマーウィジェット |
| 15 | カレンダービュー（月 / 週 / 日） |
| 16 | ガントチャート + イナズマ線（Canvas 描画） |
| 17 | 依存関係グラフ（D3.js） |
| 18 | ラベル管理 UI（エクスポート / インポート） |
| 19 | Webhook 設定画面 |

---

## 11. 決定事項ログ

| 項目 | 決定内容 | 決定日 |
|------|---------|--------|
| 削除方式 | ソフトデリート（`deleted_at`）。物理削除は管理者 API で別途提供（Phase A 対象外） | 2026-05-27 |
| 工数単位 | 分単位で DB 保存。表示時に h/m 変換 | 2026-05-27 |
| 優先順位の DB 値 | 日本語ではなく英語スラッグ（`critical_fire` / `trivial` 等）で保存 | 2026-05-27 |
| ラベルスコープ | 既存グローバルテーブルへ `project_id` カラムを追加してプロジェクト単位化 | 2026-05-27 |
| ファイル添付方式 | Drive ファイルへの参照（中間テーブル）。Drive 本体は残し、添付解除のみ | 2026-05-27 |
| Webhook シークレットの表示 | GET レスポンスではマスク（`***`）、作成時のみ平文返却 | 2026-05-27 |
| イナズマ線の描画 | Canvas API を使用。進捗率は手動入力（サブタスク自動計算は Phase 2） | 2026-05-27 |
| 実装順序 | バックエンド完全完了 → フロントエンド着手。並行実装は禁止 | 2026-05-27 |

---

## 12. 未決事項 / 今後の検討

| 項目 | 内容 |
|------|------|
| タイマーのストレージ | Redis vs DB の `timer_sessions` テーブル（現在は DB で計画） |
| 繰り返しタスク | 定期スケジュールの自動生成（週次・月次など） |
| タスクテンプレート | よく使う設定を雛形化して再利用 |
| コメント機能 | タスク上のスレッドコメント（Phase 2） |
| 通知設定の詳細化 | 各ユーザーが受け取るアラートの粒度を個別設定 |
| ガントのイナズマ線自動計算 | サブタスク完了率から進捗率を自動算出（現在は手動） |
| Webhook の署名アルゴリズム選択 | HMAC-SHA256 固定か、アルゴリズム選択を許容するか |
