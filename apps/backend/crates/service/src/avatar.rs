//! アップロードしたユーザーアバターの形式判定・ストレージキー・後片付け。

use std::sync::LazyLock;

use regex::Regex;
use sea_orm::prelude::Uuid;

use crate::storage::StorageBackend;

/// アバター画像ファイルの上限（multipart の枠は含まない）。
pub const AVATAR_MAX_BYTES: usize = 2 * 1024 * 1024;

/// 受け付ける形式の拡張子と Content-Type。
const FORMATS: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

static AVATAR_KEY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^avatar-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(png|jpg|gif|webp)$",
    )
    .expect("avatar key regex")
});

/// 先頭バイトから画像形式を判定し、`(拡張子, Content-Type)` を返す。
/// クライアント申告の content-type は信用しない。
pub fn detect_image_format(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let ext = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        return None;
    };
    FORMATS.into_iter().find(|(e, _)| *e == ext)
}

/// `avatar-{uuid}.{ext}`。local ストレージはキーに `/` を許さないのでフラットにする。
pub fn new_avatar_key(ext: &str) -> String {
    format!("avatar-{}.{ext}", Uuid::new_v4())
}

/// アバターキーとして正しい形なら、その拡張子の Content-Type を返す。
pub fn avatar_key_mime(key: &str) -> Option<&'static str> {
    let ext = AVATAR_KEY_RE.captures(key)?.get(1)?.as_str();
    FORMATS
        .into_iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, mime)| mime)
}

/// 配信 URL。`base_url` は末尾スラッシュなしの `APP_BASE_URL`。
pub fn avatar_url(base_url: &str, key: &str) -> String {
    format!("{base_url}/v1/avatars/{key}")
}

/// 使われなくなったアバター画像を消す。DB はコミット済みなので、失敗しても
/// 応答は成功のままにして孤児 object として記録だけ残す。
pub async fn delete_avatar_object(storage: &dyn StorageBackend, key: &str) {
    if let Err(e) = storage.delete(key).await {
        tracing::warn!(key, error = %e, "failed to delete avatar object");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_formats_by_magic_bytes() {
        assert_eq!(
            detect_image_format(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"),
            Some(("png", "image/png"))
        );
        assert_eq!(
            detect_image_format(b"\xFF\xD8\xFF\xE0\0\x10JFIF"),
            Some(("jpg", "image/jpeg"))
        );
        assert_eq!(
            detect_image_format(b"GIF89a\x01\0\x01\0"),
            Some(("gif", "image/gif"))
        );
        assert_eq!(
            detect_image_format(b"RIFF\x24\0\0\0WEBPVP8 "),
            Some(("webp", "image/webp"))
        );
    }

    #[test]
    fn rejects_unknown_or_empty_bytes() {
        assert_eq!(detect_image_format(b""), None);
        assert_eq!(
            detect_image_format(b"<svg xmlns=\"http://www.w3.org/2000/svg\">"),
            None
        );
        assert_eq!(detect_image_format(b"<html><script>"), None);
        // RIFF でも WEBP でなければ弾く（WAV など）
        assert_eq!(detect_image_format(b"RIFF\x24\0\0\0WAVEfmt "), None);
        // 途中で切れた RIFF ヘッダ
        assert_eq!(detect_image_format(b"RIFF\x24\0"), None);
    }

    #[test]
    fn new_key_matches_serving_pattern() {
        for (ext, mime) in FORMATS {
            assert_eq!(avatar_key_mime(&new_avatar_key(ext)), Some(mime));
        }
    }

    #[test]
    fn rejects_malformed_keys() {
        let id = "0f8fad5b-d9cb-469f-a165-70867728950e";
        assert_eq!(
            avatar_key_mime(&format!("avatar-{id}.png")),
            Some("image/png")
        );
        for bad in [
            format!("avatar-{id}.svg"),
            format!("avatar-{id}.PNG"),
            format!("avatar-{}.png", id.to_uppercase()),
            format!("avatar-{id}.png.html"),
            format!("x-avatar-{id}.png"),
            format!("avatar-{}.png", id.replace('-', "")),
            format!("../avatar-{id}.png"),
            id.to_string(),
            "avatar-.png".to_string(),
            String::new(),
        ] {
            assert_eq!(avatar_key_mime(&bad), None, "{bad}");
        }
    }
}
