---
title: タスク機能仕様
description: タスク管理システムのコア機能設計仕様書
icon: lucide:check-square
---

# タスク機能仕様書

> ステータス: **Draft**
> 作成日: 2026-05-27
> 最終更新: 2026-05-27（全機能追加）

---

## 1. 概要

プロジェクト横断で利用できるタスク管理のコア機能仕様。  
担当者・締切・工数追跡・ガントチャートを軸に、チームの作業可視化と進捗管理を実現する。  
GitHub との双方向連携により、コード変更とタスクを紐付けて開発フローを一元管理する。

ストレージ統合は既存の [Drive機能](/features/drive) を利用する。

---

## 2. スコープ

### Phase A — バックエンド（着手順）

**バックエンド完全完了後にフロントエンドへ移行する。フェーズ内の順序は下表の通り。**

| # | 機能 | 内容 |
|---|------|------|
| 1 | タスク CRUD + 連番ID | 作成・取得・更新・削除。プロジェクト内連番（#N） |
| 2 | 担当者 / 優先順位 / 締切 | Primary / Secondary 複数アサイン、Soft / Hard 締切 |
| 3 | カスタムステータス | プロジェクト単位でステータスを定義 |
| 4 | 作業時間追跡 | タイマー / 手動ログ |
| 5 | 親子・ブロッキング関係 | サブタスク階層 / blocks / blocked_by（双方向） |
| 6 | マイルストーン / スプリント | 期日管理 / 時間軸イテレーション |
| 7 | ラベル | プロジェクト単位、エクスポート / インポート |
| 8 | カスタムフィールド | プロジェクト固有の属性（ストーリーポイント等） |
| 9 | コメント / アクティビティ | スレッドコメント・@メンション・変更履歴 |
| 10 | 通知システム | in-app + メール（アサイン / メンション / 締切） |
| 11 | ウォッチャー | 担当者以外が変更を購読 |
| 12 | ファイル添付 | Drive と統合 |
| 13 | 保存済みビュー / フィルター | フィルター条件を保存・共有 |
| 14 | アーカイブ | 削除せず一覧から非表示 |
| 15 | 全文検索 | タイトル / 本文 / コメント横断検索 |
| 16 | バルク操作 | 複数タスクの一括更新 |
| 17 | 自動化（Automation） | トリガー → アクション |
| 18 | GitHub 連携 | PR / コミット / Issue との双方向リンク |
| 19 | Webhook | created / updated（差分）/ deleted |

### Phase B — フロントエンド（Phase A 完了後に着手）

| 機能 | 内容 |
|------|------|
| タスク一覧 | ボード（カンバン）/ リスト / テーブル表示 |
| タスク詳細 | コメント・添付・Relations・GitHub リンク |
| マイタスク | プロジェクト横断で自分担当タスクを表示 |
| カレンダービュー | 月 / 週 / 日の 3 モード |
| ガントチャート | イナズマ線 + スプリント表示 |
| バーンダウンチャート | スプリント / マイルストーン単位 |
| 依存関係グラフ | Relations タブ（D3.js） |
| ラベル / カスタムフィールド管理 UI | |
| 保存済みビュー UI | |
| 自動化設定 UI | |
| GitHub 連携設定 UI | |
| Webhook 設定画面 | |

---

## 3. データモデル

### 3.1 `tasks` テーブル

```rust
// entities/tasks.rs
pub struct Model {
    pub id: Uuid,
    pub project_id: Uuid,
    pub seq_id: i32,                       // プロジェクト内連番（#1, #2, …）
    pub title: String,
    pub description: Option<String>,       // Markdown テキスト
    pub status_id: Uuid,                   // → project_statuses.id
    pub priority: TaskPriority,            // enum
    pub progress_pct: i16,                 // 0–100
    pub parent_task_id: Option<Uuid>,      // 自己参照
    pub milestone_id: Option<Uuid>,
    pub sprint_id: Option<Uuid>,
    pub soft_deadline: Option<DateTimeUtc>,
    pub hard_deadline: Option<DateTimeUtc>,
    pub estimated_minutes: Option<i32>,
    pub is_archived: bool,
    pub created_by: Uuid,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `seq_id` | INT | NOT NULL | プロジェクト内連番。`project_task_counters` で採番 |
| `title` | VARCHAR(255) | NOT NULL | |
| `description` | TEXT | NULLABLE | Markdown |
| `status_id` | UUID | NOT NULL, FK→project_statuses | |
| `priority` | VARCHAR | NOT NULL DEFAULT 'medium' | `critical_fire` / `critical` / `high` / `medium` / `low` / `trivial` |
| `progress_pct` | SMALLINT | NOT NULL DEFAULT 0 CHECK (0–100) | |
| `parent_task_id` | UUID | NULLABLE, FK→tasks SET NULL | |
| `milestone_id` | UUID | NULLABLE, FK→milestones SET NULL | |
| `sprint_id` | UUID | NULLABLE, FK→sprints SET NULL | |
| `soft_deadline` | TIMESTAMPTZ | NULLABLE | |
| `hard_deadline` | TIMESTAMPTZ | NULLABLE | CONSTRAINT: soft ≤ hard |
| `estimated_minutes` | INT | NULLABLE CHECK (> 0) | |
| `is_archived` | BOOLEAN | NOT NULL DEFAULT false | |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `deleted_at` | TIMESTAMPTZ | NULLABLE | ソフトデリート |
| — | — | UNIQUE(project_id, seq_id) | |

### 3.2 `project_task_counters` テーブル

プロジェクト内連番（seq_id）をアトミックに採番するためのカウンター。

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `project_id` | UUID | PK, FK→projects CASCADE | |
| `last_seq` | INT | NOT NULL DEFAULT 0 | タスク作成時に `SELECT ... FOR UPDATE` でインクリメント |

### 3.3 `project_statuses` テーブル

プロジェクト単位でカスタマイズ可能なステータス定義。

```rust
// entities/project_statuses.rs
pub struct Model {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,               // "In Review", "QA中" など
    pub color: String,              // hex
    pub position: i16,              // 表示順
    pub is_default: bool,           // タスク作成時のデフォルト
    pub is_done_state: bool,        // このステータス = 完了とみなす
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `name` | VARCHAR(100) | NOT NULL | |
| `color` | VARCHAR(7) | NOT NULL | hex (`#3b82f6`) |
| `position` | SMALLINT | NOT NULL | ドラッグ並び替え用 |
| `is_default` | BOOLEAN | NOT NULL DEFAULT false | プロジェクト内で 1 つのみ |
| `is_done_state` | BOOLEAN | NOT NULL DEFAULT false | 複数可 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(project_id, name) | |

> **プロジェクト作成時の初期ステータス自動生成**:
> `Backlog`, `In Progress`, `In Review`, `Done` の 4 つをデフォルト挿入。
> `Backlog` が `is_default=true`、`Done` が `is_done_state=true`。

### 3.4 `task_assignees` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `role` | VARCHAR | NOT NULL DEFAULT 'secondary' | `primary` / `secondary` |
| `assigned_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(task_id, user_id) | |

### 3.5 `task_watchers` テーブル

担当者以外がタスクの変更を購読する。

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | PRIMARY KEY(task_id, user_id) | |

> 担当者へのアサイン時に自動でウォッチャー登録される。外すのは手動。

### 3.6 `task_relations` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `blocker_task_id` | UUID | NOT NULL, FK→tasks CASCADE | ブロックする側 |
| `blocked_task_id` | UUID | NOT NULL, FK→tasks CASCADE | ブロックされる側 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(blocker_task_id, blocked_task_id) | |
| — | — | CHECK(blocker_task_id <> blocked_task_id) | |

> 循環依存は追加時にサーバー側 BFS で検出し `409 Conflict`。

### 3.7 `time_logs` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `logged_minutes` | INT | NOT NULL CHECK (> 0) | |
| `logged_at` | DATE | NOT NULL | |
| `note` | TEXT | NULLABLE | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

### 3.8 `milestones` テーブル

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

### 3.9 `sprints` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `name` | VARCHAR(255) | NOT NULL | 例: "Sprint 1", "2026-W23" |
| `goal` | TEXT | NULLABLE | スプリントゴール |
| `start_date` | DATE | NOT NULL | |
| `end_date` | DATE | NOT NULL | |
| `status` | VARCHAR | NOT NULL DEFAULT 'planning' | `planning` / `active` / `completed` |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | CHECK(start_date <= end_date) | |

> プロジェクト内でアクティブなスプリント（`status='active'`）は同時に 1 つのみ。

### 3.10 `labels` テーブル（既存 + `project_id` 追加）

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | **新規追加** |
| `name` | VARCHAR(50) | NOT NULL | |
| `description` | TEXT | NOT NULL DEFAULT '' | |
| `color` | VARCHAR(7) | NOT NULL | hex |
| `icon_url` | VARCHAR | NULLABLE | |
| — | — | UNIQUE(project_id, name) | |

### 3.11 `task_labels` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `label_id` | UUID | NOT NULL, FK→labels CASCADE | |
| — | — | PRIMARY KEY(task_id, label_id) | |

### 3.12 `project_custom_fields` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `name` | VARCHAR(100) | NOT NULL | 例: "Story Points", "バージョン" |
| `field_type` | VARCHAR | NOT NULL | `text` / `number` / `select` / `date` / `url` / `checkbox` |
| `options` | JSONB | NULLABLE | `select` 型の選択肢: `[{"value":"S"},{"value":"M"}]` |
| `is_required` | BOOLEAN | NOT NULL DEFAULT false | |
| `position` | SMALLINT | NOT NULL | 表示順 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | UNIQUE(project_id, name) | |

### 3.13 `task_custom_field_values` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `field_id` | UUID | NOT NULL, FK→project_custom_fields CASCADE | |
| `value` | TEXT | NULLABLE | 全型を文字列で保持。数値・日付はアプリ層で変換 |
| — | — | PRIMARY KEY(task_id, field_id) | |

### 3.14 `task_comments` テーブル

```rust
// entities/task_comments.rs
pub struct Model {
    pub id: Uuid,
    pub task_id: Uuid,
    pub user_id: Uuid,
    pub body: String,                     // Markdown（@メンション含む）
    pub parent_comment_id: Option<Uuid>,  // スレッド返信
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub deleted_at: Option<DateTimeUtc>,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `body` | TEXT | NOT NULL | |
| `parent_comment_id` | UUID | NULLABLE, FK→task_comments | スレッド返信（1 段のみ） |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `deleted_at` | TIMESTAMPTZ | NULLABLE | ソフトデリート |

### 3.15 `task_activities` テーブル

タスクへのあらゆる変更を自動記録する監査ログ。

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `user_id` | UUID | NULLABLE, FK→users | NULL = システム操作 |
| `event_type` | VARCHAR | NOT NULL | 下表参照 |
| `payload` | JSONB | NOT NULL | 変更前後の値 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

`event_type` 一覧:

| 値 | 意味 |
|----|------|
| `task_created` | タスク作成 |
| `status_changed` | ステータス変更 |
| `priority_changed` | 優先度変更 |
| `assignee_added` / `assignee_removed` | 担当者変更 |
| `deadline_changed` | 締切変更 |
| `comment_added` | コメント投稿 |
| `relation_added` / `relation_removed` | 依存関係変更 |
| `label_added` / `label_removed` | ラベル変更 |
| `attachment_added` / `attachment_removed` | 添付変更 |
| `sprint_changed` | スプリント変更 |
| `github_pr_linked` / `github_pr_merged` | GitHub PR 連携 |
| `archived` / `unarchived` | アーカイブ |

### 3.16 `notifications` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `user_id` | UUID | NOT NULL, FK→users CASCADE | 受信者 |
| `task_id` | UUID | NULLABLE, FK→tasks | |
| `notification_type` | VARCHAR | NOT NULL | `assigned` / `mentioned` / `deadline_soon` / `status_changed` / `comment_added` / `pr_merged` |
| `payload` | JSONB | NOT NULL | 表示に必要なデータ（タスク名等） |
| `read_at` | TIMESTAMPTZ | NULLABLE | NULL = 未読 |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> 通知チャネルはメール（既存の `verification_email` ジョブを流用）と in-app（`GET /notifications`）の 2 種。

### 3.17 `notification_settings` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `user_id` | UUID | NOT NULL, FK→users CASCADE | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `email_events` | VARCHAR[] | NOT NULL DEFAULT '{}' | メール通知するイベント名 |
| `in_app_events` | VARCHAR[] | NOT NULL DEFAULT '{...}' | in-app 通知するイベント名（デフォルト全部） |
| — | — | PRIMARY KEY(user_id, project_id) | |

### 3.18 `saved_views` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `created_by` | UUID | NOT NULL, FK→users | |
| `name` | VARCHAR(100) | NOT NULL | |
| `is_shared` | BOOLEAN | NOT NULL DEFAULT false | プロジェクトメンバー全員が閲覧可 |
| `filters` | JSONB | NOT NULL | フィルター条件（下記参照） |
| `sort` | JSONB | NOT NULL DEFAULT '{}' | ソート条件 |
| `view_type` | VARCHAR | NOT NULL DEFAULT 'list' | `board` / `list` / `table` |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

`filters` JSONB 例:

```json
{
  "status_ids": ["uuid1", "uuid2"],
  "assignee_ids": ["uuid"],
  "priority": ["high", "critical"],
  "label_ids": ["uuid"],
  "sprint_id": "uuid",
  "deadline_before": "2026-07-01",
  "is_archived": false
}
```

### 3.19 `automations` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, FK→projects CASCADE | |
| `name` | VARCHAR(255) | NOT NULL | |
| `trigger` | JSONB | NOT NULL | トリガー定義（下記参照） |
| `conditions` | JSONB | NOT NULL DEFAULT '[]' | 追加条件（AND 結合） |
| `actions` | JSONB | NOT NULL | アクション定義 |
| `is_active` | BOOLEAN | NOT NULL DEFAULT true | |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

Automation JSONB スキーマ（例）:

```jsonc
// trigger
{ "event": "status_changed", "to_status_id": "uuid" }
{ "event": "assignee_added" }
{ "event": "deadline_approaching", "days_before": 1 }
{ "event": "subtask_all_done" }
{ "event": "github_pr_merged" }

// conditions（任意）
[
  { "field": "priority", "op": "eq", "value": "critical" }
]

// actions（複数可）
[
  { "type": "set_status", "status_id": "uuid" },
  { "type": "add_label", "label_id": "uuid" },
  { "type": "assign_user", "user_id": "uuid" },
  { "type": "set_progress", "value": 100 },
  { "type": "notify_assignees" },
  { "type": "post_comment", "body": "自動クローズしました。" }
]
```

### 3.20 `github_integrations` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `project_id` | UUID | NOT NULL, UNIQUE, FK→projects CASCADE | |
| `installation_id` | BIGINT | NOT NULL | GitHub Apps のインストール ID |
| `repo_owner` | VARCHAR | NOT NULL | 例: `myorg` |
| `repo_name` | VARCHAR | NOT NULL | 例: `myapp` |
| `access_token_enc` | TEXT | NOT NULL | 暗号化済み Installation Access Token |
| `token_expires_at` | TIMESTAMPTZ | NOT NULL | 期限（1 時間）。期限切れ時は自動再取得 |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

### 3.21 `task_github_links` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `link_type` | VARCHAR | NOT NULL | `pull_request` / `commit` / `branch` |
| `github_repo` | VARCHAR | NOT NULL | `owner/repo` 形式 |
| `github_number` | INT | NULLABLE | PR / Issue 番号 |
| `github_sha` | VARCHAR(40) | NULLABLE | コミット SHA |
| `title` | VARCHAR | NOT NULL | PR タイトル / コミットメッセージ先頭行 |
| `github_url` | VARCHAR | NOT NULL | GitHub 上の URL |
| `state` | VARCHAR | NULLABLE | `open` / `closed` / `merged`（PR のみ） |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| `updated_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

### 3.22 `task_attachments` テーブル

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `task_id` | UUID | NOT NULL, FK→tasks CASCADE | |
| `drive_file_id` | UUID | NOT NULL, FK→drive_files CASCADE | |
| `attached_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |
| — | — | PRIMARY KEY(task_id, drive_file_id) | |

### 3.23 `webhooks` / `webhook_deliveries` テーブル

（既出のため割愛。[セクション 3 元定義参照](#)）

---

## 4. アクセス制御

### 4.1 アクセスルール

| 操作 | 権限 |
|------|------|
| タスク閲覧 / 検索 | プロジェクトメンバー **または** テナントオーナー |
| タスク作成 / 更新 / アーカイブ | プロジェクトメンバー **または** テナントオーナー |
| タスク削除（ソフト） | 作成者 / テナントオーナー |
| コメント作成 | プロジェクトメンバー **または** テナントオーナー |
| コメント削除 | 投稿者本人 / テナントオーナー |
| カスタムフィールド定義 | テナントオーナーのみ |
| 自動化設定 | テナントオーナーのみ |
| GitHub 連携設定 | テナントオーナーのみ |
| Webhook 設定 | テナントオーナーのみ |

### 4.2 スコープ

```rust
pub enum Scope {
    // 既存
    ReadProject, WriteProject,
    // 追加
    ReadTask, WriteTask,
    ReadMilestone, WriteMilestone,
    ManageWebhook,
    ManageAutomation,
    ManageGitHub,
}
```

---

## 5. マイグレーション

```sql
-- project_task_counters
CREATE TABLE project_task_counters (
    project_id UUID PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    last_seq INT NOT NULL DEFAULT 0
);

-- project_statuses
CREATE TABLE project_statuses (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    color VARCHAR(7) NOT NULL,
    position SMALLINT NOT NULL,
    is_default BOOLEAN NOT NULL DEFAULT false,
    is_done_state BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

-- tasks
CREATE TABLE tasks (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    seq_id INT NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    status_id UUID NOT NULL REFERENCES project_statuses(id),
    priority VARCHAR NOT NULL DEFAULT 'medium',
    progress_pct SMALLINT NOT NULL DEFAULT 0 CHECK (progress_pct BETWEEN 0 AND 100),
    parent_task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
    milestone_id UUID REFERENCES milestones(id) ON DELETE SET NULL,
    sprint_id UUID REFERENCES sprints(id) ON DELETE SET NULL,
    soft_deadline TIMESTAMPTZ,
    hard_deadline TIMESTAMPTZ,
    estimated_minutes INT CHECK (estimated_minutes > 0),
    is_archived BOOLEAN NOT NULL DEFAULT false,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    UNIQUE (project_id, seq_id),
    CONSTRAINT soft_before_hard CHECK (
        soft_deadline IS NULL OR hard_deadline IS NULL OR soft_deadline <= hard_deadline
    )
);

-- sprints（tasks より先に作成）
CREATE TABLE sprints (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    goal TEXT,
    start_date DATE NOT NULL,
    end_date DATE NOT NULL,
    status VARCHAR NOT NULL DEFAULT 'planning',
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (start_date <= end_date)
);

-- task_assignees / task_watchers / task_relations / time_logs
-- task_labels / task_attachments （前出のため省略）

-- project_custom_fields
CREATE TABLE project_custom_fields (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    field_type VARCHAR NOT NULL,
    options JSONB,
    is_required BOOLEAN NOT NULL DEFAULT false,
    position SMALLINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, name)
);

-- task_custom_field_values
CREATE TABLE task_custom_field_values (
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    field_id UUID NOT NULL REFERENCES project_custom_fields(id) ON DELETE CASCADE,
    value TEXT,
    PRIMARY KEY (task_id, field_id)
);

-- task_comments
CREATE TABLE task_comments (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    parent_comment_id UUID REFERENCES task_comments(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

-- task_activities
CREATE TABLE task_activities (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    event_type VARCHAR NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- notifications
CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    task_id UUID REFERENCES tasks(id) ON DELETE CASCADE,
    notification_type VARCHAR NOT NULL,
    payload JSONB NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- notification_settings
CREATE TABLE notification_settings (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    email_events VARCHAR[] NOT NULL DEFAULT '{}',
    in_app_events VARCHAR[] NOT NULL DEFAULT
        '{assigned,mentioned,deadline_soon,comment_added,pr_merged}',
    PRIMARY KEY (user_id, project_id)
);

-- saved_views
CREATE TABLE saved_views (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id),
    name VARCHAR(100) NOT NULL,
    is_shared BOOLEAN NOT NULL DEFAULT false,
    filters JSONB NOT NULL DEFAULT '{}',
    sort JSONB NOT NULL DEFAULT '{}',
    view_type VARCHAR NOT NULL DEFAULT 'list',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- automations
CREATE TABLE automations (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    trigger JSONB NOT NULL,
    conditions JSONB NOT NULL DEFAULT '[]',
    actions JSONB NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- github_integrations
CREATE TABLE github_integrations (
    id UUID PRIMARY KEY,
    project_id UUID NOT NULL UNIQUE REFERENCES projects(id) ON DELETE CASCADE,
    installation_id BIGINT NOT NULL,
    repo_owner VARCHAR NOT NULL,
    repo_name VARCHAR NOT NULL,
    access_token_enc TEXT NOT NULL,
    token_expires_at TIMESTAMPTZ NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- task_github_links
CREATE TABLE task_github_links (
    id UUID PRIMARY KEY,
    task_id UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    link_type VARCHAR NOT NULL,
    github_repo VARCHAR NOT NULL,
    github_number INT,
    github_sha VARCHAR(40),
    title VARCHAR NOT NULL,
    github_url VARCHAR NOT NULL,
    state VARCHAR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- 全文検索インデックス（PostgreSQL tsvector）
ALTER TABLE tasks
    ADD COLUMN search_vector tsvector
    GENERATED ALWAYS AS (
        to_tsvector('japanese', coalesce(title, '') || ' ' || coalesce(description, ''))
    ) STORED;
CREATE INDEX idx_tasks_search ON tasks USING GIN(search_vector);
CREATE INDEX idx_tasks_project_id ON tasks(project_id) WHERE deleted_at IS NULL;
CREATE INDEX idx_tasks_status ON tasks(status_id);
CREATE INDEX idx_tasks_sprint ON tasks(sprint_id) WHERE sprint_id IS NOT NULL;
CREATE INDEX idx_task_activities_task ON task_activities(task_id, created_at DESC);
CREATE INDEX idx_notifications_user_unread ON notifications(user_id, created_at DESC)
    WHERE read_at IS NULL;
```

---

## 6. API 設計

全エンドポイントはセッション認証または PAT 認証が必須。  
URL 基底パス: `/v1/tenants/{tenant_id}/projects/{project_id}`

### 6.1 タスク API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks` | タスク一覧（フィルター・ソート・ページング） |
| `POST` | `/tasks` | タスク作成 |
| `GET` | `/tasks/{id}` | タスク取得（`#N` 形式の seq_id でも検索可） |
| `PUT` | `/tasks/{id}` | タスク更新（全フィールド optional） |
| `DELETE` | `/tasks/{id}` | ソフトデリート |
| `POST` | `/tasks/{id}/archive` | アーカイブ |
| `POST` | `/tasks/{id}/unarchive` | アーカイブ解除 |
| `POST` | `/tasks/bulk` | バルク操作 |
| `GET` | `/tasks/search` | 全文検索 |

#### GET `/tasks` クエリパラメータ

| パラメータ | 型 | 説明 |
|-----------|-----|------|
| `status_id` | UUID[]? | カスタムステータスでフィルター |
| `priority` | string[]? | |
| `assignee_id` | UUID? | |
| `milestone_id` | UUID? | |
| `sprint_id` | UUID? | |
| `label_id` | UUID? | |
| `parent_task_id` | UUID? | サブタスク一覧 |
| `is_archived` | bool | デフォルト false |
| `view_id` | UUID? | 保存済みビューのフィルターを適用 |
| `sort` | string | `created_at_desc` / `priority_asc` / `deadline_asc` 等 |
| `limit` / `offset` | u32 | デフォルト 50、最大 200 |

#### POST `/tasks/bulk` バルク操作

```json
{
  "task_ids": ["uuid1", "uuid2", "uuid3"],
  "operations": [
    { "type": "set_status", "status_id": "uuid" },
    { "type": "add_label", "label_id": "uuid" },
    { "type": "set_assignee", "user_id": "uuid", "role": "secondary" }
  ]
}
```

対応する `type`: `set_status` / `set_priority` / `add_label` / `remove_label` /  
`set_assignee` / `set_sprint` / `set_milestone` / `archive` / `delete`

#### GET `/tasks/search` 全文検索

```
GET /tasks/search?q=ログイン&limit=20
```

対象: `tasks.title`, `tasks.description`, `task_comments.body`  
レスポンス: タスク一覧（スコア降順）

### 6.2 担当者 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/assignees` | 一覧 |
| `POST` | `/tasks/{id}/assignees` | 追加 |
| `PUT` | `/tasks/{id}/assignees/{user_id}` | ロール変更 |
| `DELETE` | `/tasks/{id}/assignees/{user_id}` | 削除 |

### 6.3 ウォッチャー API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/watchers` | 購読者一覧 |
| `POST` | `/tasks/{id}/watchers` | 自分を追加（`user_id` 不要、認証ユーザー） |
| `DELETE` | `/tasks/{id}/watchers/{user_id}` | 解除 |

### 6.4 コメント API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/comments` | コメント一覧（スレッド構造） |
| `POST` | `/tasks/{id}/comments` | 投稿（`parent_comment_id` で返信） |
| `PUT` | `/tasks/{id}/comments/{comment_id}` | 編集 |
| `DELETE` | `/tasks/{id}/comments/{comment_id}` | ソフトデリート |

```json
POST /tasks/{id}/comments
{
  "body": "この件は @鈴木さん に確認してください。",
  "parent_comment_id": null
}
```

> `@ユーザー名` のメンションはサーバー側でパースし、対象ユーザーへ通知を生成する。

### 6.5 アクティビティ API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/activities` | 変更履歴一覧（新着順） |

レスポンス例:

```json
{
  "activities": [
    {
      "id": "uuid",
      "event_type": "status_changed",
      "user": { "id": "uuid", "name": "田中" },
      "payload": { "from": "Backlog", "to": "In Review" },
      "created_at": "2026-05-27T11:00:00Z"
    }
  ]
}
```

### 6.6 作業時間追跡 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/time-logs` | ログ一覧 |
| `POST` | `/tasks/{id}/time-logs` | 手動ログ追加 |
| `DELETE` | `/tasks/{id}/time-logs/{log_id}` | 削除 |
| `POST` | `/tasks/{id}/timer/start` | タイマー開始 |
| `POST` | `/tasks/{id}/timer/stop` | タイマー停止 → ログ生成 |
| `GET` | `/tasks/{id}/time-logs/summary` | 工数サマリー |

### 6.7 親子・依存関係 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/relations` | 一覧（subtasks / blocks / blocked_by） |
| `POST` | `/tasks/{id}/relations` | 追加 |
| `DELETE` | `/tasks/{id}/relations/{relation_id}` | 削除 |

#### ブロッキング関係の方向性

`type` フィールドで方向を指定する。**どちらのタスクからでも設定可能**。

| `type` | 意味 | 作成されるレコード |
|--------|------|-------------------|
| `"blocks"` | 現タスクが `target` をブロック | `blocker=現タスク`, `blocked=target` |
| `"blocked_by"` | 現タスクは `target` にブロックされている | `blocker=target`, `blocked=現タスク` |

例: タスク A がタスク B をブロックする場合、**どちら側からでも同一レコードを作成できる**。

```json
// タスク A の画面から
POST /tasks/{task_a_id}/relations
{ "type": "blocks", "target_task_id": "{task_b_id}" }

// タスク B の画面から（同じ結果）
POST /tasks/{task_b_id}/relations
{ "type": "blocked_by", "target_task_id": "{task_a_id}" }
```

循環依存・重複は `409 Conflict`。

### 6.8 ファイル添付 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/tasks/{id}/attachments` | 一覧 |
| `POST` | `/tasks/{id}/attachments` | Drive ファイルを紐付け |
| `DELETE` | `/tasks/{id}/attachments/{file_id}` | 紐付け解除（Drive 本体は残る） |

### 6.9 カスタムステータス API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/statuses` | プロジェクトのステータス一覧（position 順） |
| `POST` | `/statuses` | 作成 |
| `PUT` | `/statuses/{status_id}` | 更新（名称 / 色 / is_done_state） |
| `PUT` | `/statuses/reorder` | 並び順を一括更新 |
| `DELETE` | `/statuses/{status_id}` | 削除（タスクが存在する場合は移行先 status_id を指定） |

`DELETE` リクエスト:

```json
{ "migrate_to_status_id": "uuid" }
```

### 6.10 カスタムフィールド API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/custom-fields` | フィールド定義一覧 |
| `POST` | `/custom-fields` | 作成 |
| `PUT` | `/custom-fields/{field_id}` | 更新 |
| `DELETE` | `/custom-fields/{field_id}` | 削除（値も CASCADE） |
| `PUT` | `/tasks/{id}/custom-fields` | タスクのフィールド値を一括更新 |

`PUT /tasks/{id}/custom-fields` リクエスト:

```json
{
  "values": [
    { "field_id": "uuid", "value": "5" },
    { "field_id": "uuid2", "value": "v2.0" }
  ]
}
```

### 6.11 マイルストーン API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/milestones` | 一覧 |
| `POST` | `/milestones` | 作成 |
| `GET` | `/milestones/{id}` | 取得（完了率含む） |
| `PUT` | `/milestones/{id}` | 更新 |
| `DELETE` | `/milestones/{id}` | 削除（タスクの milestone_id は NULL リセット） |

### 6.12 スプリント API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/sprints` | 一覧 |
| `POST` | `/sprints` | 作成 |
| `GET` | `/sprints/{id}` | 取得（バーンダウンデータ含む） |
| `PUT` | `/sprints/{id}` | 更新 |
| `POST` | `/sprints/{id}/start` | スプリント開始（`status → active`） |
| `POST` | `/sprints/{id}/complete` | 完了（未完了タスクの移動先を指定） |

`POST /sprints/{id}/complete` リクエスト:

```json
{
  "move_incomplete_to_sprint_id": "uuid",  // 次スプリントへ移動
  "move_incomplete_to_backlog": false       // または Backlog へ
}
```

`GET /sprints/{id}` バーンダウンデータ:

```json
{
  "id": "uuid",
  "name": "Sprint 3",
  "start_date": "2026-06-01",
  "end_date": "2026-06-14",
  "status": "active",
  "burndown": [
    { "date": "2026-06-01", "ideal_remaining": 80, "actual_remaining": 80 },
    { "date": "2026-06-02", "ideal_remaining": 74, "actual_remaining": 70 },
    { "date": "2026-06-03", "ideal_remaining": 68, "actual_remaining": 75 }
  ],
  "task_counts": { "total": 20, "done": 5, "in_progress": 8 }
}
```

### 6.13 ラベル API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/labels` | プロジェクトのラベル一覧 |
| `POST` | `/labels` | 作成 |
| `PUT` | `/labels/{id}` | 更新 |
| `DELETE` | `/labels/{id}` | 削除 |
| `GET` | `/labels/export` | JSON エクスポート |
| `POST` | `/labels/import` | JSON インポート（`on_conflict`: `skip` / `overwrite`） |

### 6.14 通知 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/users/me/notifications` | 自分の通知一覧（未読優先） |
| `POST` | `/v1/users/me/notifications/read-all` | 全件既読 |
| `PUT` | `/v1/users/me/notifications/{id}/read` | 1 件既読 |
| `GET` | `/notification-settings` | プロジェクト単位の通知設定取得 |
| `PUT` | `/notification-settings` | 通知設定更新 |

### 6.15 保存済みビュー API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/views` | 一覧（自分のビュー + 共有ビュー） |
| `POST` | `/views` | 作成 |
| `PUT` | `/views/{id}` | 更新 |
| `DELETE` | `/views/{id}` | 削除 |

### 6.16 自動化 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/automations` | 一覧 |
| `POST` | `/automations` | 作成 |
| `PUT` | `/automations/{id}` | 更新 |
| `DELETE` | `/automations/{id}` | 削除 |
| `POST` | `/automations/{id}/toggle` | 有効 / 無効切替 |

### 6.17 マイタスク API

プロジェクト横断で自分にアサインされたタスクを返す。

```
GET /v1/users/me/tasks?status=open&limit=50
```

### 6.18 GitHub 連携 API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/github/integration` | 連携状態取得 |
| `POST` | `/github/integration` | 連携設定（installation_id + repo） |
| `DELETE` | `/github/integration` | 連携解除 |
| `GET` | `/tasks/{id}/github-links` | タスクに紐付いた PR / コミット一覧 |
| `POST` | `/tasks/{id}/github-links` | 手動リンク追加 |
| `DELETE` | `/tasks/{id}/github-links/{link_id}` | リンク削除 |
| `POST` | `/v1/github/webhook` | GitHub からの Webhook 受信（公開エンドポイント） |

---

## 7. GitHub 連携詳細

### 7.1 認証方式

**GitHub Apps** を使用する（個人 OAuth トークンではなくインストールベース）。

- テナントオーナーが GitHub App をリポジトリにインストール
- インストール後、GitHub からリダイレクトで `installation_id` を受け取り DB に保存
- Installation Access Token（有効期限 1 時間）を定期的に再取得してキャッシュ

```
GitHub App インストールフロー:
  1. テナントオーナーが設定画面の「GitHub と連携」をクリック
  2. GitHub App のインストールページへリダイレクト
  3. リポジトリを選択してインストール
  4. GitHub がコールバック URL へリダイレクト:
     /v1/github/callback?installation_id=12345&code=xxxxx
  5. バックエンドが installation_id を DB に保存
  6. 設定完了
```

### 7.2 タスクへの自動リンク

PR タイトル・ブランチ名・コミットメッセージに `#N`（プロジェクト内連番）を含めると自動でリンクされる。

| パターン | 例 | 動作 |
|---------|-----|------|
| コミットメッセージ | `fix: ログインバグを修正 #42` | タスク #42 にコミットリンク追加 |
| PR タイトル | `feat: OAuth 対応 #42 #43` | タスク #42, #43 に PR リンク追加 |
| ブランチ名 | `feat/task-42-oauth` | タスク #42 にブランチリンク追加 |
| クローズキーワード | `fix: ... Closes #42` / `Fixes #42` / `Resolves #42` | PR マージ時にタスク #42 を完了ステータスへ自動遷移 |

> **クローズキーワード**: `Closes` / `Fixes` / `Resolves`（大小文字不問）。PR 本文でも有効。

### 7.3 受信する GitHub Webhook イベント

| GitHub イベント | 処理内容 |
|---------------|---------|
| `push` | コミットメッセージをパースし `task_github_links` に追加 |
| `pull_request.opened` | PR タイトル / 本文をパースし PR リンクを追加 |
| `pull_request.edited` | リンクを再解析・更新 |
| `pull_request.closed`（merged=true） | リンク先タスクの `state` を `merged` に更新。`Closes #N` キーワードがあれば完了ステータスへ自動遷移 + Automation トリガー発火 |
| `pull_request.closed`（merged=false） | `state` を `closed` に更新 |
| `pull_request.reopened` | `state` を `open` に更新 |
| `create`（ref_type=branch） | ブランチ名をパースしリンク追加 |

GitHub Webhook のシークレット検証（HMAC-SHA256 / `X-Hub-Signature-256`）を必ず実施する。

### 7.4 タスク詳細での表示

タスク詳細画面の「GitHub」タブに以下を表示:

```
┌─────────────────────────────────────────────┐
│ GitHub                                       │
├─────────────────────────────────────────────┤
│ Pull Requests                                │
│  ✅ #87 feat: OAuth対応    [Merged]          │
│  🔄 #91 fix: エラーハンドリング  [Open]      │
│                                             │
│ Commits                                     │
│  a3f92c1  fix: トークン期限切れを修正         │
│  e7b81d4  feat: GitHub App 初期実装           │
│                                             │
│ Branches                                    │
│  feat/task-42-oauth                         │
│                                [+ 手動リンク] │
└─────────────────────────────────────────────┘
```

### 7.5 GitHub 側での表示

GitHub PR / Issue の本文に自動でタスクへのリンクを追記する（GitHub App の権限 `pull_requests: write` が必要）。

```
---
🔗 Linked Task: [#42 OAuth対応](https://app.example.com/projects/.../tasks/42)
```

---

## 8. Webhook ペイロード仕様

リクエストヘッダー: `X-Task-Event`, `X-Task-Signature: sha256=<HMAC-SHA256>`

### task.created / task.updated / task.deleted

（前出の定義と同様。`task.updated` は変更フィールドの差分のみ）

### 追加イベント

| イベント | トリガー |
|---------|--------|
| `task.archived` / `task.unarchived` | アーカイブ操作 |
| `comment.created` | コメント投稿 |
| `github.pr_linked` | PR リンク追加 |
| `github.pr_merged` | PR マージ |
| `sprint.started` / `sprint.completed` | スプリント状態変化 |

リトライ: 指数バックオフで最大 5 回（30s / 5m / 30m / 2h）。5 回失敗で自動無効化。

---

## 9. フロントエンド UI 設計

**Phase B で実装。Phase A（バックエンド）完全完了後に着手すること。**

### 9.1 ページ構成（vike + Vue）

```
/tenants/{tid}/projects/{pid}/tasks           # タスク一覧
/tenants/{tid}/projects/{pid}/tasks/{id}      # タスク詳細
/tenants/{tid}/projects/{pid}/calendar        # カレンダービュー
/tenants/{tid}/projects/{pid}/gantt           # ガントチャート
/tenants/{tid}/projects/{pid}/sprints         # スプリント管理
/tenants/{tid}/projects/{pid}/milestones      # マイルストーン
/tenants/{tid}/projects/{pid}/labels          # ラベル管理
/tenants/{tid}/projects/{pid}/custom-fields   # カスタムフィールド
/tenants/{tid}/projects/{pid}/automations     # 自動化設定
/tenants/{tid}/projects/{pid}/settings/github # GitHub 連携
/tenants/{tid}/projects/{pid}/settings/webhooks # Webhook
/v1/users/me/tasks                            # マイタスク（横断）
```

### 9.2 タスク詳細レイアウト

```
┌───────────────────────────────────────────────────────────────────┐
│ ← プロジェクト名  /  #42 OAuth対応         [アーカイブ] [削除]   │
├─────────────────────────────────┬─────────────────────────────────┤
│                                 │ ステータス  [In Review ▼]       │
│  # OAuth 対応を実装する          │ 優先度  🔴 High                 │
│                                 │ 進捗率  [████░░░░] 50%          │
│  ## 概要                        │                                 │
│  GitHub Apps を使って…          │ 担当者                          │
│                                 │   🧑 田中（Primary）            │
│  ─ カスタムフィールド ─         │   🧑 鈴木（Secondary）          │
│  Story Points: [5]              │ ウォッチャー  🧑 佐藤            │
│  バージョン: [v2.0]             │                                 │
│                                 │ 仮締め  2026-06-01              │
│  📎 添付  🖼 screen.png         │ Deadline  2026-06-10            │
│                                 │ 見積  3h / 実績  1h30m ▶        │
│  ─────────────────────         │                                 │
│  💬 コメント                    │ ラベル  [feature] [auth]        │
│  田中: 設計は完了しました        │ マイルストーン  v2.0            │
│  > 鈴木: レビュー依頼します      │ スプリント  Sprint 3            │
│                                 │                                 │
│  ─────────────────────         │ ─────────────────────────────── │
│  📋 アクティビティ              │ GitHub                          │
│  田中が優先度を変更: medium→high │   ✅ #87 feat: OAuth  [Merged]  │
│  システムが PR #87 をリンク      │   🔄 #91 fix: エラー  [Open]    │
│                                 │   branch: feat/task-42-oauth    │
│  🔗 Relations                   │                                 │
│  blocks: #55                    │ ─────────────────────────────── │
│  subtasks: #60 #61              │ サブタスク                      │
│  [グラフで表示]                  │   □ #60 JWT実装                 │
│                                 │   □ #61 テスト                  │
└─────────────────────────────────┴─────────────────────────────────┘
```

### 9.3 カンバンボード（ステータス列）

```
┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐
│ Backlog  │  │In Progress│ │ In Review│  │   Done   │
├──────────┤  ├──────────┤  ├──────────┤  ├──────────┤
│ #42      │  │ #38      │  │ #30      │  │ #25 ✅   │
│ OAuth    │  │ DB設計    │  │ ログイン  │  │ 環境構築  │
│ 🔴High   │  │ 🟡Medium  │  │ 🔴High   │  │          │
├──────────┤  ├──────────┤  ├──────────┤  ├──────────┤
│ #43      │  │          │  │          │  │ #20 ✅   │
│ テスト   │  │          │  │          │  │          │
└──────────┘  └──────────┘  └──────────┘  └──────────┘
  [+ ステータス追加]
```

カスタムステータスはドラッグで列を並び替え可能。

### 9.4 バーンダウンチャート

```
残タスク数
80│╲
  │  ╲  ideal
60│    ╲─────
  │     ╲   ╲ actual（遅延）
40│      ──╲──╲──
  │          ╲   ╲
20│            ──── ← 現在地
  │
 0└─────────────────── 日付
   6/1         6/14
```

### 9.5 コンポーネント構成

| コンポーネント | ファイル | 説明 |
|--------------|---------|------|
| `TaskListPage` | `pages/tasks/+Page.vue` | ボード / リスト / テーブル切替 |
| `TaskDetailPage` | `pages/tasks/[id]/+Page.vue` | 詳細 |
| `TaskDetailPanel` | `components/tasks/TaskDetailPanel.vue` | 右ペイン |
| `TaskComments` | `components/tasks/TaskComments.vue` | コメントスレッド |
| `TaskActivities` | `components/tasks/TaskActivities.vue` | 変更履歴 |
| `TaskGitHubPanel` | `components/tasks/TaskGitHubPanel.vue` | GitHub タブ |
| `AssigneeSelector` | `components/tasks/AssigneeSelector.vue` | |
| `WatcherList` | `components/tasks/WatcherList.vue` | |
| `TimerWidget` | `components/tasks/TimerWidget.vue` | |
| `RelationsGraph` | `components/tasks/RelationsGraph.vue` | D3.js |
| `CustomFieldValues` | `components/tasks/CustomFieldValues.vue` | |
| `KanbanBoard` | `components/tasks/KanbanBoard.vue` | ステータス列 |
| `CalendarPage` | `pages/calendar/+Page.vue` | |
| `GanttPage` | `pages/gantt/+Page.vue` | |
| `ProgressLine` | `components/gantt/ProgressLine.vue` | イナズマ線（Canvas） |
| `BurndownChart` | `components/sprint/BurndownChart.vue` | |
| `SprintPage` | `pages/sprints/+Page.vue` | |
| `AutomationPage` | `pages/automations/+Page.vue` | |
| `AutomationEditor` | `components/automations/AutomationEditor.vue` | トリガー / アクション設定 UI |
| `GitHubSettingsPage` | `pages/settings/github/+Page.vue` | |
| `NotificationBell` | `components/layout/NotificationBell.vue` | ヘッダーの通知アイコン |
| `MyTasksPage` | `pages/me/tasks/+Page.vue` | プロジェクト横断マイタスク |
| `LabelManager` | `pages/labels/+Page.vue` | |
| `CustomFieldManager` | `pages/custom-fields/+Page.vue` | |
| `SavedViewManager` | `components/tasks/SavedViewManager.vue` | |

---

## 10. セキュリティ

| 脅威 | 対策 |
|------|------|
| 他プロジェクトのタスクへのアクセス | 全エンドポイントで `project_id` 所属チェック |
| GitHub App トークンの漏洩 | DB では暗号化保存（AES-256-GCM）。API レスポンスでは返さない |
| GitHub Webhook の偽装 | `X-Hub-Signature-256` を HMAC-SHA256 で検証。不一致は即 `403` |
| コメント内 XSS | フロントエンドは `DOMPurify` でサニタイズ |
| 自動化の無限ループ | Automation 実行は同一タスクに対し 5 秒以内の再発火を抑制 |
| seq_id の競合 | `SELECT last_seq FROM project_task_counters WHERE project_id=? FOR UPDATE` でアトミック採番 |
| 循環依存 | `task_relations` 追加時に BFS で事前チェック |
| Webhook シークレット露出 | GET レスポンスでは `***` にマスク。作成時のみ平文返却 |

---

## 11. 実装方針

**バックエンド完全完了後にフロントエンドへ移行する。**

### Phase A — バックエンド

| # | 内容 | 完了条件 |
|---|------|---------|
| 1 | マイグレーション（全テーブル） | `migration run` 正常完了 |
| 2 | エンティティ定義 | コンパイル通過 |
| 3 | タスク CRUD + seq_id 採番 | Scalar で動作確認 |
| 4 | カスタムステータス CRUD | 同上 |
| 5 | 担当者 / ウォッチャー / 優先順位 / 締切 | 同上 |
| 6 | 作業時間追跡（タイマー含む） | 同上 |
| 7 | 親子・ブロッキング関係（循環検出含む） | 同上 |
| 8 | スプリント CRUD + 開始 / 完了 | バーンダウンデータ取得確認 |
| 9 | マイルストーン CRUD | 同上 |
| 10 | ラベル CRUD + export / import | 同上 |
| 11 | カスタムフィールド定義 + 値 CRUD | 同上 |
| 12 | コメント CRUD + @メンションパース | 同上 |
| 13 | アクティビティ自動記録 | タスク更新時に自動生成確認 |
| 14 | 通知システム（in-app + メール） | アサイン時に通知生成確認 |
| 15 | ファイル添付（Drive 統合） | 同上 |
| 16 | バルク操作 | 同上 |
| 17 | 全文検索 | `tsvector` インデックス有効確認 |
| 18 | 保存済みビュー CRUD | 同上 |
| 19 | 自動化エンジン | トリガー → アクション発火確認 |
| 20 | GitHub App 連携 + Webhook 受信 | PR マージ → タスク自動クローズ確認 |
| 21 | Webhook 送信基盤 | 同上 |
| 22 | `pnpm openapi` 型エラーなし | — |

### Phase B — フロントエンド（Phase A 完了後に着手）

| # | 内容 |
|---|------|
| 23 | タスク一覧（カンバン / リスト / テーブル） |
| 24 | タスク詳細（コメント・アクティビティ・GitHub タブ） |
| 25 | 担当者・ウォッチャー・タイマー UI |
| 26 | カスタムフィールド入力 UI |
| 27 | カレンダービュー |
| 28 | ガントチャート + イナズマ線 |
| 29 | スプリント管理 + バーンダウンチャート |
| 30 | 依存関係グラフ（D3.js） |
| 31 | 通知ベル + 通知一覧 |
| 32 | マイタスクダッシュボード |
| 33 | 保存済みビュー UI |
| 34 | 自動化設定エディタ |
| 35 | GitHub 連携設定・PR/コミット表示 |
| 36 | ラベル / カスタムフィールド管理 |
| 37 | Webhook 設定画面 |

---

## 12. 決定事項ログ

| 項目 | 決定内容 | 決定日 |
|------|---------|--------|
| 削除方式 | ソフトデリート（`deleted_at`）。物理削除は管理者 API（Phase A 対象外） | 2026-05-27 |
| 工数単位 | 分単位で DB 保存。表示時に h/m 変換 | 2026-05-27 |
| 優先順位の DB 値 | 英語スラッグ（`critical_fire` / `trivial` 等）で保存 | 2026-05-27 |
| ステータス | ハードコードせずプロジェクト単位のカスタムステータス（`project_statuses` テーブル）で管理 | 2026-05-27 |
| タスク連番 ID | `project_task_counters` で `SELECT ... FOR UPDATE` によるアトミック採番。`#N` 形式で表示 | 2026-05-27 |
| ラベルスコープ | 既存グローバルテーブルへ `project_id` を追加してプロジェクト単位化 | 2026-05-27 |
| ファイル添付方式 | Drive ファイルへの参照（中間テーブル）。Drive 本体は残し、添付解除のみ | 2026-05-27 |
| GitHub 連携方式 | GitHub Apps（インストールベース）を採用。個人 OAuth トークンは使わない | 2026-05-27 |
| Webhook シークレット表示 | GET レスポンスではマスク（`***`）。作成時のみ平文返却 | 2026-05-27 |
| イナズマ線の描画 | Canvas API。進捗率は手動入力（サブタスク自動計算は Phase 2） | 2026-05-27 |
| Automation の無限ループ対策 | 同一タスクへの 5 秒以内の再発火を抑制 | 2026-05-27 |
| スプリント同時稼働 | プロジェクト内でアクティブスプリントは同時 1 つのみ | 2026-05-27 |
| 実装順序 | バックエンド完全完了 → フロントエンド着手。並行実装は禁止 | 2026-05-27 |

---

## 13. 未決事項 / 今後の検討

| 項目 | 内容 |
|------|------|
| タイマーのストレージ | Redis vs DB の `timer_sessions` テーブル |
| 繰り返しタスク | 定期スケジュールの自動生成（週次・月次など） |
| タスクテンプレート | よく使う設定を雛形化して再利用 |
| モバイル対応 | カレンダー・ガントのモバイル表示最適化 |
| GitLab 対応 | GitHub 連携と同様の仕組みで GitLab にも対応するか |
| CSV インポート | 他ツール（Jira / Asana）からの移行 |
| コメントへの絵文字リアクション | |
| ストーリーポイント集計 | スプリント単位のベロシティ計算 |
| 全文検索エンジン | PostgreSQL tsvector で十分か、Meilisearch 等を別途立てるか |
