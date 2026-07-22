//! Drive クォータ・設定ヘルパー。

use std::env;
use std::path::Path;

use sea_orm::prelude::Uuid;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, ExprTrait, QueryFilter,
    QuerySelect,
};

use crate::error::AppError;
use entity::{drive_files, tenants};

/// Drive 関連の環境変数設定。
#[derive(Clone, Debug)]
pub struct DriveConfig {
    pub upload_max_bytes: u64,
    /// `0` = 無制限（デフォルトクォータ未設定時）
    pub default_quota_bytes: i64,
    /// `0` = 天井なし
    pub system_max_quota_bytes: i64,
}

impl DriveConfig {
    pub fn from_env() -> Self {
        let upload_max_mb = env_u64("UPLOAD_MAX_SIZE_MB", 100);
        let default_quota_mb = env_i64("DRIVE_DEFAULT_QUOTA_MB", 10240);
        let system_max_mb = env_i64("DRIVE_SYSTEM_MAX_QUOTA_MB", 51200);

        Self {
            upload_max_bytes: upload_max_mb.saturating_mul(1024 * 1024),
            default_quota_bytes: mb_to_bytes(default_quota_mb),
            system_max_quota_bytes: mb_to_bytes(system_max_mb),
        }
    }

    pub fn system_max_bytes_opt(&self) -> Option<i64> {
        if self.system_max_quota_bytes == 0 {
            None
        } else {
            Some(self.system_max_quota_bytes)
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn mb_to_bytes(mb: i64) -> i64 {
    if mb == 0 {
        0
    } else {
        mb.saturating_mul(1024 * 1024)
    }
}

/// テナントの有効クォータ（バイト）。`None` = 無制限。
/// システム上限（`system_max_quota_bytes`）が設定されている場合は常にその値でキャップする。
pub fn effective_quota(tenant: &tenants::Model, config: &DriveConfig) -> Option<i64> {
    let requested = tenant
        .drive_quota_bytes
        .unwrap_or(config.default_quota_bytes);
    let requested_opt = if requested == 0 {
        None
    } else {
        Some(requested)
    };
    let system_max_opt = config.system_max_bytes_opt();
    match (requested_opt, system_max_opt) {
        (Some(q), Some(max)) => Some(std::cmp::Ord::min(q, max)),
        (None, Some(max)) => Some(max),
        (Some(q), None) => Some(q),
        (None, None) => None,
    }
}

pub async fn tenant_used_bytes<C: ConnectionTrait>(
    db: &C,
    tenant_id: Uuid,
) -> Result<i64, AppError> {
    // Postgres の SUM(bigint) は NUMERIC を返すため、そのまま i64 で受け取ると
    // 「NUMERIC は INT8 と互換でない」というデコードエラーになる。行が 0 件のときは
    // SUM が NULL になり素通りするので、テナントにファイルが 1 件でもあると再現する。
    // BIGINT へキャストしてから受け取る。
    let sum = drive_files::Entity::find()
        .filter(drive_files::Column::TenantId.eq(tenant_id))
        .select_only()
        .column_as(
            sea_orm::sea_query::Expr::col(drive_files::Column::Size)
                .sum()
                .cast_as("bigint"),
            "total",
        )
        .into_tuple::<Option<i64>>()
        .one(db)
        .await?;

    Ok(sum.flatten().unwrap_or(0))
}

pub fn current_storage_type() -> entity::drive_files::StorageType {
    match env::var("STORAGE_BACKEND")
        .unwrap_or_else(|_| "local".into())
        .as_str()
    {
        "s3" => entity::drive_files::StorageType::S3,
        _ => entity::drive_files::StorageType::Local,
    }
}

/// テナントオーナー判定（drive_files / drive_folders の共通ヘルパー）。
pub async fn is_tenant_owner(
    db: &DatabaseConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> Result<bool, AppError> {
    let tenant = tenants::Entity::find_by_id(tenant_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(tenant.owner_id == user_id)
}

/// `mime_guess` がソースコードとして解決できない拡張子の上書き表。
///
/// `mime_guess` は `.ts` を MPEG-TS（`video/vnd.dlna.mpeg-tts`）に解決し、
/// `.go` や `.rb` などは未定義のため `application/octet-stream` に落ちる。
/// そのままだとテキストとして扱えず編集エンドポイントが弾いてしまうため、
/// ここで `text/*` に寄せる。`mime_guess` が既に text 系を返す拡張子
/// （`.rs` / `.py` / `.c` / `.lua` など）は重複させない。
const SOURCE_MIME_OVERRIDES: &[(&str, &str)] = &[
    ("cjs", "text/javascript"),
    ("clj", "text/x-clojure"),
    ("dart", "text/x-dart"),
    ("erl", "text/x-erlang"),
    ("ex", "text/x-elixir"),
    ("exs", "text/x-elixir"),
    ("go", "text/x-go"),
    ("graphql", "text/x-graphql"),
    ("java", "text/x-java"),
    ("kt", "text/x-kotlin"),
    ("nim", "text/x-nim"),
    ("proto", "text/x-protobuf"),
    ("rb", "text/x-ruby"),
    ("scala", "text/x-scala"),
    ("svelte", "text/x-svelte"),
    ("swift", "text/x-swift"),
    ("ts", "text/typescript"),
    ("tsx", "text/typescript"),
    ("vue", "text/x-vue"),
    ("zig", "text/x-zig"),
];

/// 上書き表に載っているソースコード拡張子の MIME を返す。
///
/// クライアントが申告する Content-Type よりこちらを優先したい場面があるため、
/// [`guess_mime`] とは別に公開している。ブラウザは `.ts` を `video/mp2t` の
/// ように申告することがあり、そのまま保存すると本文編集エンドポイントが弾く。
pub fn source_mime_override(filename: &str) -> Option<&'static str> {
    let extension = Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)?;

    SOURCE_MIME_OVERRIDES
        .iter()
        .find(|(ext, _)| *ext == extension)
        .map(|(_, mime)| *mime)
}

pub fn guess_mime(filename: &str) -> String {
    if let Some(mime) = source_mime_override(filename) {
        return mime.to_string();
    }

    mime_guess::from_path(filename)
        .first_raw()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "application/octet-stream".into())
}

/// エディタで本文を差し替えられるテキスト系 MIME か判定する。
///
/// `text/*` に加えて、構造化サフィックス（`+json` / `+xml`。`application/ld+json` や
/// `image/svg+xml` が該当）と、テキストでありながら `application/*` に置かれている
/// 形式を許可する。`text/plain; charset=utf-8` のようなパラメータ付きの値も受け付ける。
pub fn is_editable_mime(mime: &str) -> bool {
    let essence = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if essence.starts_with("text/") || essence.ends_with("+json") || essence.ends_with("+xml") {
        return true;
    }

    matches!(
        essence.as_str(),
        "application/ecmascript"
            | "application/graphql"
            | "application/javascript"
            | "application/json"
            | "application/toml"
            | "application/x-httpd-php"
            | "application/x-sh"
            | "application/x-sql"
            | "application/x-toml"
            | "application/x-yaml"
            | "application/xml"
            | "application/yaml"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mime_guess` 単体では誤り（`.ts` = MPEG-TS）または未定義になる拡張子が、
    /// 上書き表によってテキストとして解決されることの回帰ガード。
    #[test]
    fn guess_mime_resolves_source_extensions_as_text() {
        for name in [
            "main.ts",
            "App.tsx",
            "main.go",
            "app.rb",
            "Main.java",
            "main.kt",
            "schema.graphql",
            "Component.vue",
        ] {
            let mime = guess_mime(name);
            assert!(
                is_editable_mime(&mime),
                "{name} は編集可能なテキストとして解決されるべき (実際: {mime})"
            );
        }
        assert_eq!(guess_mime("main.ts"), "text/typescript");
        assert_eq!(guess_mime("MAIN.GO"), "text/x-go");
    }

    /// 上書き表に無い拡張子は従来どおり `mime_guess` の結果を返す。
    #[test]
    fn guess_mime_falls_back_to_mime_guess() {
        assert_eq!(guess_mime("lib.rs"), "text/x-rust");
        assert_eq!(guess_mime("notes.md"), "text/markdown");
        assert_eq!(guess_mime("data.json"), "application/json");
        assert_eq!(guess_mime("photo.png"), "image/png");
        assert_eq!(guess_mime("archive"), "application/octet-stream");
    }

    #[test]
    fn is_editable_mime_accepts_text_and_text_like_types() {
        for mime in [
            "text/plain",
            "text/plain; charset=utf-8",
            "  TEXT/Markdown  ",
            "text/x-rust",
            "application/json",
            "application/ld+json",
            "image/svg+xml",
            "application/x-sh",
            "application/x-yaml",
        ] {
            assert!(is_editable_mime(mime), "{mime} は編集可能であるべき");
        }
    }

    #[test]
    fn is_editable_mime_rejects_binary_types() {
        for mime in [
            "image/png",
            "application/pdf",
            "application/octet-stream",
            "video/vnd.dlna.mpeg-tts",
            "application/zip",
            "",
        ] {
            assert!(!is_editable_mime(mime), "{mime} は編集不可であるべき");
        }
    }
}
