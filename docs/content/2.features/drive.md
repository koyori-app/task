---
title: Drive 機能仕様
description: Misskey ライクなドライブ機能の設計仕様書
icon: lucide:hard-drive
---

# Drive 機能仕様書

> ステータス: **Draft — 仕様更新中**
> 作成日: 2026-05-26
> 最終更新: 2026-05-26（アクセス制御追加）

---

## 1. 概要

タスク管理 SaaS に Misskey ライクなドライブ機能を追加する。
ドライブはテナント単位のファイル管理スペースであり、アップロードしたファイルをフォルダで整理し、タスクへの添付や共有 URL 発行に利用できる。

ストレージバックエンドは **S3 互換（AWS S3 / MinIO）** と **ローカルディスク** の 2 種類をサポートし、環境変数で切り替える。

---

## 2. スコープ

### Phase 1（MVP）— 本仕様書の対象

| 機能 | 内容 |
|------|------|
| ファイルアップロード | multipart/form-data で受付。S3 または ローカルへ保存 |
| ファイル一覧・取得 | テナント内ファイルの一覧取得、メタデータ取得 |
| ファイル削除 | ストレージ上のオブジェクトも同時削除 |
| フォルダ CRUD | 階層フォルダの作成・取得・削除 |
| 直接 URL | ローカルストレージの場合はバックエンド経由でファイルを配信 |
| フロントエンド UI | ファイルブラウザ、アップロード UI（shadcn/ui） |

### Phase 2 以降（本仕様外）

- ファイルをタスクへ添付
- ファイル共有リンクの有効期限設定
- 画像サムネイル生成
- ファイル全文検索

---

## 3. データモデル

### 3.1 `drive_folders` テーブル

```rust
// entities/drive_folders.rs
pub struct Model {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,   // 自己参照（ルートフォルダは None）
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,  // プロジェクト紐付き（設定時はプロジェクトフォルダ）
    pub created_by: Uuid,          // FK → users
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `name` | VARCHAR | NOT NULL | フォルダ名 |
| `parent_id` | UUID | NULLABLE, FK→self | 親フォルダ（ルートは NULL） |
| `tenant_id` | UUID | NOT NULL, FK→tenants CASCADE | |
| `project_id` | UUID | NULLABLE, FK→projects CASCADE | セットされているとプロジェクトフォルダ |
| `created_by` | UUID | NOT NULL, FK→users | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> **プロジェクトフォルダの自動作成**: プロジェクト作成時に同名フォルダを `drive_folders` に自動作成し、`project_id` を紐付ける。プロジェクト削除時は CASCADE で削除される。

### 3.2 `drive_files` テーブル

```rust
// entities/drive_files.rs
pub struct Model {
    pub id: Uuid,
    pub name: String,              // 表示名（元ファイル名）
    pub size: i64,                 // バイト数
    pub mime_type: String,         // application/octet-stream 等
    pub storage_type: StorageType, // enum: s3 | local
    pub storage_key: String,       // S3 key または ローカル相対パス
    pub url: String,               // アクセス URL（アクセス制御ルールに基づく）
    pub tenant_id: Uuid,
    pub project_id: Option<Uuid>,  // 非正規化。フォルダの project_id を引き継ぐ
    pub uploader_id: Uuid,         // FK → users
    pub folder_id: Option<Uuid>,   // FK → drive_folders
    pub created_at: DateTimeUtc,
}
```

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `id` | UUID | PK | |
| `name` | VARCHAR | NOT NULL | ファイル表示名 |
| `size` | BIGINT | NOT NULL | バイト数 |
| `mime_type` | VARCHAR | NOT NULL | |
| `storage_type` | ENUM | NOT NULL | `s3` または `local` |
| `storage_key` | VARCHAR | NOT NULL | ストレージ固有キー |
| `url` | VARCHAR | NOT NULL | アクセス URL（下記「アクセス制御」参照） |
| `tenant_id` | UUID | NOT NULL, FK→tenants CASCADE | |
| `project_id` | UUID | NULLABLE | フォルダの `project_id` を非正規化コピー。アクセス制御の高速判定に使用 |
| `uploader_id` | UUID | NOT NULL, FK→users | |
| `folder_id` | UUID | NULLABLE, FK→drive_folders | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

> **`project_id` の非正規化**: アクセス制御を毎回フォルダ階層を辿らず O(1) で判定するため、ファイルにもフォルダの `project_id` を保持する。ファイルのフォルダ移動時は `project_id` を再設定する。

### 3.3 `tenants` テーブルへの追加カラム

既存テーブルに以下のカラムを追加する。

| カラム | 型 | 制約 | 説明 |
|--------|-----|------|------|
| `drive_quota_bytes` | BIGINT | NULLABLE | テナントのドライブ最大容量（バイト）。`NULL` = システムデフォルト適用 |

```rust
// entities/tenants.rs に追加
pub drive_quota_bytes: Option<i64>, // NULL = システムデフォルト（DRIVE_DEFAULT_QUOTA_MB 参照）
```

---

## 4. アクセス制御

### 4.1 アクセスルール

| ファイルの種類 | 閲覧できる人 |
|--------------|------------|
| `project_id = NULL`（一般ファイル） | リンクを知っていれば誰でも（認証不要） |
| `project_id = <id>`（プロジェクトファイル） | そのプロジェクトのメンバー **または** テナントオーナー |

### 4.2 プロジェクトメンバー判定

既存の `project_members` テーブルを参照する。テナントオーナーは `tenants.owner_id` で判定。

```rust
// アクセス可否チェックの擬似コード
fn can_access_file(file: &DriveFile, user: &User) -> bool {
    match file.project_id {
        None => true, // 一般ファイル: 誰でもアクセス可
        Some(project_id) => {
            // プロジェクトメンバー OR テナントオーナー
            is_project_member(user.id, project_id)
                || is_tenant_owner(user.id, file.tenant_id)
        }
    }
}
```

### 4.3 URL 戦略

| ファイルの種類 | `url` フィールドの値 | 認証 |
|--------------|---------------------|------|
| 一般ファイル（S3） | S3 パブリック URL or CDN URL | 不要 |
| 一般ファイル（ローカル） | `/v1/drive/files/{id}/content` | 不要 |
| プロジェクトファイル（S3） | `/v1/drive/files/{id}/content`（バックエンドが S3 からプロキシ） | **必要** |
| プロジェクトファイル（ローカル） | `/v1/drive/files/{id}/content` | **必要** |

S3 バックエンドでもプロジェクトファイルはバックエンドがプロキシするため、S3 バケットのオブジェクト URL をクライアントに直接渡さない。バケット自体はパブリックでよいが、プロジェクトファイルの S3 キーは直接露出しない。

### 4.4 プロジェクトフォルダのライフサイクル

| イベント | 動作 |
|---------|------|
| プロジェクト作成 | `drive_folders` にプロジェクトフォルダを自動作成（`project_id` セット） |
| プロジェクト削除 | CASCADE により `drive_folders` → `drive_files` の順で削除。ストレージオブジェクトも連動削除 |
| ファイルをプロジェクトフォルダへ移動 | `drive_files.project_id` を移動先フォルダの `project_id` に更新 |
| ファイルをプロジェクトフォルダ外へ移動 | `drive_files.project_id` を `NULL` にリセット |

---

## 5. クォータ管理

### 5.1 クォータの仕組み

| 優先度 | 設定元 | 値 |
|--------|--------|-----|
| 1（最優先） | `tenants.drive_quota_bytes` | テナント個別の上限（バイト）|
| 2（フォールバック） | 環境変数 `DRIVE_DEFAULT_QUOTA_MB` | 全テナント共通のデフォルト上限 |
| 3（無制限） | `DRIVE_DEFAULT_QUOTA_MB=0` | `0` を指定すると無制限 |

テナント個別の `drive_quota_bytes` が `NULL` の場合はシステムデフォルトを適用する。

### 5.2 使用量の計算

使用量はアップロード時に `drive_files` テーブルを集計して算出する（キャッシュなし、常に正確な値）。

```sql
SELECT COALESCE(SUM(size), 0) FROM drive_files WHERE tenant_id = $1
```

アップロード前に「現在の使用量 + 新ファイルのサイズ ≤ クォータ」を検証し、超過する場合は `413 Content Too Large` を返す。

### 5.3 クォータ取得 API

```
GET /v1/tenants/{tenant_id}/drive/usage
```

レスポンス:

```json
{
  "used_bytes": 524288000,
  "quota_bytes": 10737418240,
  "unlimited": false
}
```

| フィールド | 説明 |
|-----------|------|
| `used_bytes` | 現在の使用量（バイト） |
| `quota_bytes` | 有効なクォータ（バイト）。`unlimited: true` の場合は `null` |
| `unlimited` | `DRIVE_DEFAULT_QUOTA_MB=0` かつ `drive_quota_bytes=NULL` の場合 `true` |

### 5.4 クォータ設定 API

テナントオーナーがドライブ容量を変更できる。

```
PATCH /v1/tenants/{tenant_id}/drive/quota
```

リクエスト:

```json
{ "quota_bytes": 10737418240 }
```

- `null` を渡すとシステムデフォルトにリセット
- テナントオーナー権限が必要（既存の `ensure_tenant_owner` を流用）

---

## 6. ストレージバックエンド

### 6.1 抽象インターフェース（Rust trait）

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn upload(&self, key: &str, data: Bytes, mime: &str) -> Result<String, StorageError>;
    async fn delete(&self, key: &str) -> Result<(), StorageError>;
    fn public_url(&self, key: &str) -> String;
}
```

### 6.2 S3 バックエンド

- クレート: `aws-sdk-s3`
- S3 互換エンドポイントに対応（MinIO / Cloudflare R2 / Backblaze B2）
- バケットのパブリックアクセスポリシーを前提に公開 URL を生成

```env
STORAGE_BACKEND=s3
S3_ENDPOINT=https://s3.amazonaws.com      # MinIO: http://localhost:9000
S3_BUCKET=my-task-drive
S3_REGION=ap-northeast-1
S3_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE
S3_SECRET_ACCESS_KEY=wJalrXUtnFEMI...
S3_PUBLIC_BASE_URL=https://cdn.example.com  # CDN 前段を置く場合（省略時はエンドポイント+バケット名）
S3_FORCE_PATH_STYLE=true                  # MinIO 等で必要。false がデフォルト
```

> **`S3_FORCE_PATH_STYLE`**: AWS S3 は仮想ホスト形式（`bucket.s3.amazonaws.com`）が標準だが、MinIO などのセルフホスト互換では `http://endpoint/bucket/key` 形式（パス形式）が必要。`true` に設定すると `aws-sdk-s3` の `force_path_style` オプションが有効になる。

**アップロードフロー（S3）**:
1. クライアント → `POST /v1/tenants/{id}/drive/files` (multipart)
2. バックエンドがストリームを受信
3. `aws-sdk-s3` で `PutObject` (ファイルサイズ < 5MB) または `CreateMultipartUpload` (≥ 5MB)
4. DB に `drive_files` レコードを登録
5. レスポンスに `url` を含めて返却

### 6.3 ローカルバックエンド

- 環境変数 `LOCAL_UPLOAD_DIR` で保存先ディレクトリを指定
- バックエンドが `GET /v1/drive/files/{id}/content` でファイルを配信
- 開発環境・セルフホスト向け

```env
STORAGE_BACKEND=local
LOCAL_UPLOAD_DIR=/var/task/uploads
```

**ファイル配信エンドポイント（ローカル・S3 プロジェクトファイル共通）**:

```
GET /v1/drive/files/{id}/content
```

- **一般ファイル（`project_id = NULL`）**: 認証不要。リンクを知っていれば誰でもアクセス可
- **プロジェクトファイル（`project_id` あり）**: セッション or PAT 認証必須。プロジェクトメンバーまたはテナントオーナーのみ。それ以外は `403 Forbidden`
- `Content-Type` ヘッダーを `mime_type` から設定
- `Content-Disposition: inline` で配信（画像はブラウザで表示）
- S3 バックエンドの場合: バックエンドが S3 からオブジェクトをストリーム取得してプロキシ

---

## 7. API 設計

全エンドポイントはセッション認証 or PAT 認証が必須（既存の `auth` ミドルウェアを流用）。

### 7.1 ファイル API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/tenants/{tenant_id}/drive/files` | ファイル一覧 |
| `POST` | `/v1/tenants/{tenant_id}/drive/files` | ファイルアップロード |
| `GET` | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイルメタデータ取得 |
| `PATCH` | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイル更新（名前・フォルダ移動） |
| `DELETE` | `/v1/tenants/{tenant_id}/drive/files/{id}` | ファイル削除 |
| `GET` | `/v1/drive/files/{id}/content` | ファイル内容配信（ローカル・S3 プロキシ） |
| `GET` | `/v1/tenants/{tenant_id}/drive/usage` | 使用量・クォータ取得 |
| `PATCH` | `/v1/tenants/{tenant_id}/drive/quota` | クォータ設定（テナントオーナーのみ） |

#### GET `/v1/tenants/{tenant_id}/drive/files`

クエリパラメータ:

| パラメータ | 型 | デフォルト | 説明 |
|-----------|-----|----------|------|
| `folder_id` | UUID? | - | フォルダ絞り込み（省略時はルート） |
| `limit` | u32 | 50 | 取得件数（最大 200） |
| `offset` | u32 | 0 | オフセット |

レスポンス:

```json
{
  "files": [
    {
      "id": "...",
      "name": "screenshot.png",
      "size": 204800,
      "mime_type": "image/png",
      "url": "https://cdn.example.com/...",
      "folder_id": null,
      "created_at": "2026-05-26T12:00:00Z"
    }
  ],
  "total": 42
}
```

#### POST `/v1/tenants/{tenant_id}/drive/files`

リクエスト: `multipart/form-data`

| フィールド | 必須 | 説明 |
|-----------|------|------|
| `file` | ✓ | ファイルバイナリ |
| `name` | - | 表示名（省略時は元ファイル名） |
| `folder_id` | - | アップロード先フォルダ UUID |

レスポンス: 作成された `DriveFile` オブジェクト (201 Created)

制限:
- 最大ファイルサイズ: 環境変数 `UPLOAD_MAX_SIZE_MB` で設定（デフォルト 100MB）
- 許可 MIME タイプ: 全種類

### 7.2 フォルダ API

| メソッド | パス | 説明 |
|---------|------|------|
| `GET` | `/v1/tenants/{tenant_id}/drive/folders` | フォルダ一覧 |
| `POST` | `/v1/tenants/{tenant_id}/drive/folders` | フォルダ作成 |
| `PATCH` | `/v1/tenants/{tenant_id}/drive/folders/{id}` | フォルダ更新（名前変更・移動） |
| `DELETE` | `/v1/tenants/{tenant_id}/drive/folders/{id}` | フォルダ削除 |

フォルダ削除時の挙動:
- フォルダ内ファイルが存在する場合: **`409 Conflict`** を返し削除しない（強制削除は Phase 2 以降）

---

## 8. フロントエンド UI 設計

### 8.1 ページ構成

```
/tenants/{tenant_id}/drive       # ドライブトップ（ルートフォルダ）
/tenants/{tenant_id}/drive/folders/{folder_id}  # フォルダ内
```

### 8.2 レイアウト

```
┌─────────────────────────────────────────────────────┐
│ Breadcrumb: ドライブ > フォルダA > サブフォルダB      │
├────────────────┬────────────────────────────────────┤
│                │  ┌──────────────────────────────┐  │
│ [+ 新しい      │  │ 🔍 ファイル検索               │  │
│   フォルダ]    │  └──────────────────────────────┘  │
│                │                                    │
│ ▼ ドライブ     │  [▲ アップロード]  [リスト/グリッド] │
│   フォルダA    │                                    │
│   フォルダB    │  📁 フォルダA    📁 フォルダB       │
│               │  📄 report.pdf  🖼 image.png        │
└────────────────┴────────────────────────────────────┘
```

### 8.3 コンポーネント構成

| コンポーネント | ファイル | 説明 |
|--------------|---------|------|
| `DrivePage` | `pages/tenants/[id]/drive/index.vue` | ドライブ全体 |
| `DriveSidebar` | `components/drive/DriveSidebar.vue` | フォルダツリー |
| `DriveFileGrid` | `components/drive/DriveFileGrid.vue` | ファイル一覧（グリッド） |
| `DriveFileList` | `components/drive/DriveFileList.vue` | ファイル一覧（リスト） |
| `DriveUploadButton` | `components/drive/DriveUploadButton.vue` | ドラッグ&ドロップ対応 |
| `DriveFileCard` | `components/drive/DriveFileCard.vue` | ファイルカード（グリッド用） |
| `DriveFileDetail` | `components/drive/DriveFileDetail.vue` | 詳細パネル（サイドシート） |

### 8.4 主要インタラクション

- **アップロード**: ボタンクリック or エリアへドラッグ&ドロップ → プログレスバー表示
- **フォルダ作成**: サイドバーの「+ 新しいフォルダ」ボタン → インライン入力
- **ファイル詳細**: ファイルカードをクリック → 右サイドシートで詳細表示・URL コピー
- **削除**: 右クリックメニュー or 詳細パネルの削除ボタン → 確認ダイアログ

---

## 9. セキュリティ

| 脅威 | 対策 |
|------|------|
| 他テナントのファイルへのアクセス | 全エンドポイントで `tenant_id` の所有権チェック |
| プロジェクト外ユーザーによるファイルアクセス | `drive_files.project_id` で判定し、非メンバーは `403` |
| S3 の直接 URL によるプロジェクトファイルの漏洩 | プロジェクトファイルの S3 URL はクライアントに渡さない。バックエンドプロキシ経由のみ |
| 任意ファイル上書き | ストレージキーは UUID v4 で生成（衝突なし） |
| 超大型ファイルによる DoS | `axum` の `DefaultBodyLimit` を `UPLOAD_MAX_SIZE_MB` から動的設定 |
| パストラバーサル（ローカル） | ストレージキーは UUID のみ使用。元ファイル名はメタデータのみ |

---

## 10. 設定まとめ

`apps/backend/.env` に追加する環境変数:

```env
# ストレージバックエンド（必須）
STORAGE_BACKEND=local                 # "local" または "s3"

# アップロード・クォータ設定（共通）
UPLOAD_MAX_SIZE_MB=100                # 1ファイルあたりの上限 MB（デフォルト 100）
DRIVE_DEFAULT_QUOTA_MB=10240          # テナントデフォルト容量 MB（デフォルト 10GB）。0 = 無制限

# S3 用（STORAGE_BACKEND=s3 の場合）
S3_ENDPOINT=https://s3.amazonaws.com
S3_BUCKET=
S3_REGION=ap-northeast-1
S3_ACCESS_KEY_ID=
S3_SECRET_ACCESS_KEY=
S3_PUBLIC_BASE_URL=                   # CDN を使う場合（省略時はエンドポイント+バケット名）
S3_FORCE_PATH_STYLE=false             # MinIO 等では true に設定

# ローカル用（STORAGE_BACKEND=local の場合）
LOCAL_UPLOAD_DIR=./uploads
```

---

## 11. 実装計画

### ブランチ名
```
feat/drive
```

### タスク分解

**実装順序: バックエンド完全完了後にフロントエンドへ移行する。**

#### Phase A — バックエンド

| # | 内容 |
|---|------|
| 1 | `StorageBackend` trait + Local 実装 |
| 2 | S3 実装（`aws-sdk-s3`、`force_path_style` 対応） |
| 3 | `tenants` に `drive_quota_bytes` カラム追加 |
| 4 | `drive_folders` エンティティ + API（`project_id` 含む） |
| 5 | `drive_files` エンティティ + API（`project_id` 含む、アップロード時クォータ検証） |
| 6 | ファイル配信エンドポイント（アクセス制御付き、S3 プロキシ対応） |
| 7 | クォータ取得・設定 API（`GET/PATCH /drive/usage`, `/drive/quota`） |
| 8 | プロジェクト作成フック — ドライブフォルダ自動作成 |
| 9 | Bruno で全エンドポイントの動作確認（Phase A 完了条件） |

#### Phase B — フロントエンド（Phase A 完了後に着手）

| # | 内容 |
|---|------|
| 10 | `pnpm openapi` でクライアント再生成 |
| 11 | `DrivePage` + `DriveSidebar`（プロジェクトフォルダを識別表示） |
| 12 | `DriveFileGrid` + `DriveUploadButton`（ドラッグ&ドロップ） |
| 13 | `DriveFileDetail` サイドシート |
| 14 | クォータ使用量バー（ドライブ画面下部に表示） |

---

## 12. 決定事項ログ

| 項目 | 決定内容 | 決定日 |
|------|---------|--------|
| ファイルサイズ上限 | `UPLOAD_MAX_SIZE_MB` 環境変数で設定変更可（デフォルト 100MB） | 2026-05-26 |
| 一般ファイルの配信認証 | 認証不要。リンクを知っていれば誰でもアクセス可 | 2026-05-26 |
| プロジェクトファイルのアクセス制御 | プロジェクトメンバーまたはテナントオーナーのみ閲覧可 | 2026-05-26 |
| プロジェクトファイルの URL 戦略 | バックエンド経由でプロキシ（S3 URL は直接渡さない） | 2026-05-26 |
| S3 バケット公開設定 | パブリックバケット（プロジェクトファイルのキーは露出しない） | 2026-05-26 |
| フォルダ削除の挙動 | ファイルが存在する場合は `409 Conflict`（強制削除は Phase 2） | 2026-05-26 |
| MIME タイプ制限 | Phase 1 は全種類許可 | 2026-05-26 |
| S3 ForcePathStyle | `S3_FORCE_PATH_STYLE` 環境変数で設定可（MinIO 等向け） | 2026-05-26 |
| プロジェクトフォルダ自動作成 | プロジェクト作成時に対応するドライブフォルダを自動作成 | 2026-05-26 |
| テナント別ドライブ容量 | `tenants.drive_quota_bytes` で個別設定。`NULL` 時は `DRIVE_DEFAULT_QUOTA_MB` 適用（`0` = 無制限） | 2026-05-26 |
| クォータ超過時のレスポンス | `413 Content Too Large` | 2026-05-26 |
