use chrono::{DateTime, Utc};
use sea_orm::prelude::Uuid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;
use validator::Validate;

use entity::task_comments;

use crate::mentions::{MentionMap, MentionedUser};
use crate::reactions::ReactionSummary;

#[derive(Debug, Clone, Serialize, ToSchema, serde::Deserialize)]
pub struct TaskCommentResponse {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,
    /// 本文。メンションは `<@利用者ID>` の印のまま返す（画面が札に描く・編集で開き直すため）。
    /// 利用者の画面の外へ出す時は `mentions` で名に戻す（`payload::mentions::to_plain`）。
    pub body: String,
    /// 本文の印の id から引ける表示情報（名前・アイコン・退会や対象外かの印）。
    /// 描く時に API を叩き直さずに済むよう、本文と並べて返す。
    #[serde(default)]
    #[schema(value_type = HashMap<String, MentionedUser>)]
    pub mentions: MentionMap,
    #[schema(value_type = Option<String>, format = "uuid", nullable)]
    pub parent_comment_id: Option<Uuid>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTime<Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: DateTime<Utc>,
    #[schema(value_type = Option<String>, format = "date-time", nullable)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl From<task_comments::Model> for TaskCommentResponse {
    fn from(model: task_comments::Model) -> Self {
        Self {
            id: model.id,
            task_id: model.task_id,
            user_id: model.user_id,
            body: model.body,
            mentions: MentionMap::new(),
            parent_comment_id: model.parent_comment_id,
            created_at: model.created_at.with_timezone(&Utc),
            updated_at: model.updated_at.with_timezone(&Utc),
            deleted_at: model.deleted_at.map(|dt| dt.with_timezone(&Utc)),
        }
    }
}

#[derive(Serialize, serde::Deserialize, ToSchema)]
pub struct CommentUser {
    #[schema(required = false)]
    pub account_kind: String,
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
    /// 画面でアイコンを出すために返す。未設定なら頭文字で描く
    #[schema(nullable)]
    pub avatar_url: Option<String>,
}

impl CommentUser {
    /// id → (名前, アイコン, 利用者の種別) の表から作る。名前が引けない場合も表示は続けたいので
    /// `unknown` へ倒す（種別は人として扱う）。
    pub fn from_lookup(
        map: &HashMap<Uuid, (String, Option<String>, String)>,
        user_id: Uuid,
    ) -> Self {
        let (name, avatar_url, account_kind) = map
            .get(&user_id)
            .cloned()
            .unwrap_or_else(|| ("unknown".into(), None, "human".into()));
        Self {
            account_kind,
            id: user_id,
            name,
            avatar_url,
        }
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CommentReply {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub user: CommentUser,
    #[schema(nullable)]
    pub body: Option<String>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
    /// 絵文字リアクション（最初に付いた順）。削除済みコメントは空
    // CLI は使わない項目なので、無い応答も読む（openapi は変えないので `required` は残す）
    #[serde(default)]
    #[schema(required)]
    pub reactions: Vec<ReactionSummary>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CommentThread {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub user: CommentUser,
    #[schema(nullable)]
    pub body: Option<String>,
    pub replies: Vec<CommentReply>,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[schema(value_type = String, format = "date-time")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub is_deleted: bool,
    /// 絵文字リアクション（最初に付いた順）。削除済みコメントは空
    // CLI は使わない項目なので、無い応答も読む（openapi は変えないので `required` は残す）
    #[serde(default)]
    #[schema(required)]
    pub reactions: Vec<ReactionSummary>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CommentListResponse {
    pub comments: Vec<CommentThread>,
    /// 全コメントの本文に出る印の id から引ける表示情報（削除済みのコメントの本文は含めぬ）。
    #[schema(value_type = HashMap<String, MentionedUser>)]
    pub mentions: MentionMap,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityUser {
    #[schema(required = false)]
    pub account_kind: String,
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub name: String,
}

#[derive(Serialize, ToSchema)]
pub struct ActivityItem {
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    pub event_type: String,
    #[schema(nullable)]
    pub user: Option<ActivityUser>,
    #[schema(value_type = serde_json::Value)]
    pub payload: serde_json::Value,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 履歴の取得範囲。
///
/// 履歴は操作のたびに増えるので、既定で先頭だけ返す。全件返すと
/// 長く使われたタスクほど DB・レスポンス・描画のコストが上限なく伸びる。
#[derive(Deserialize, ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListActivitiesQuery {
    #[serde(default = "default_activities_limit")]
    pub limit: u64,
    /// 前のページの `next_cursor`。先頭ページでは付けない。
    ///
    /// `offset` を使わないのは、履歴が積まれている最中にページを継ぐと境界がずれ、
    /// 同じ行が 2 度出たり抜けたりするため（`common::cursor`）。
    pub cursor: Option<String>,
}

fn default_activities_limit() -> u64 {
    20
}

/// 上限。これより大きい `limit` は切り詰める。
pub const MAX_ACTIVITIES_LIMIT: u64 = 100;

#[derive(Serialize, ToSchema)]
pub struct ActivityListResponse {
    pub activities: Vec<ActivityItem>,
    /// 総数。件数の表示に使う。**「まだ残っているか」の判断には使わない**
    /// （取得中に増えるので、総数と取得済み件数の比較では終わらなくなる）
    pub total: u64,
    /// 次のページを引く鍵。`null` なら取り切っている
    #[schema(required, nullable)]
    pub next_cursor: Option<String>,
}

#[derive(Validate, Deserialize, ToSchema, serde::Serialize)]
pub struct CreateCommentRequest {
    #[validate(length(min = 1))]
    pub body: String,
    #[schema(value_type = Option<String>, format = "uuid")]
    pub parent_comment_id: Option<Uuid>,
}

#[derive(Validate, Deserialize, ToSchema)]
pub struct UpdateCommentRequest {
    #[validate(length(min = 1))]
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> CommentUser {
        CommentUser {
            account_kind: "human".into(),
            id: Uuid::nil(),
            name: "alice".into(),
            avatar_url: None,
        }
    }

    /// CLI は一覧の応答を同じ型で読む。直列化したものが読み戻せる。
    #[test]
    fn the_comment_list_reads_back_what_it_serializes() {
        let now = chrono::Utc::now();
        let list = CommentListResponse {
            comments: vec![CommentThread {
                id: Uuid::nil(),
                user: user(),
                body: Some("本文".into()),
                replies: vec![CommentReply {
                    id: Uuid::nil(),
                    user: user(),
                    body: None,
                    created_at: now,
                    updated_at: now,
                    is_deleted: true,
                    reactions: vec![],
                }],
                created_at: now,
                updated_at: now,
                is_deleted: false,
                reactions: vec![],
            }],
            mentions: MentionMap::new(),
        };

        let json = serde_json::to_value(&list).unwrap();
        let back: CommentListResponse = serde_json::from_value(json.clone()).unwrap();

        assert_eq!(serde_json::to_value(&back).unwrap(), json);
    }

    /// CLI が使わない `reactions` が無い応答でも読める（版のずれで止まらない）。
    #[test]
    fn a_comment_without_reactions_is_still_readable() {
        let json = serde_json::json!({
            "comments": [{
                "id": Uuid::nil(),
                "user": { "account_kind": "human", "id": Uuid::nil(), "name": "a", "avatar_url": null },
                "body": "x",
                "replies": [{
                    "id": Uuid::nil(),
                    "user": { "account_kind": "human", "id": Uuid::nil(), "name": "a", "avatar_url": null },
                    "body": "y",
                    "created_at": "2026-01-01T00:00:00Z",
                    "updated_at": "2026-01-01T00:00:00Z",
                    "is_deleted": false,
                }],
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
                "is_deleted": false,
            }],
            "mentions": {},
        });

        let list: CommentListResponse = serde_json::from_value(json).unwrap();

        assert!(list.comments[0].reactions.is_empty());
        assert!(list.comments[0].replies[0].reactions.is_empty());
    }
}
