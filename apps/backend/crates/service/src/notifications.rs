use sea_orm::entity::prelude::Json;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    prelude::Uuid,
};
use std::collections::HashSet;

use crate::error::AppError;
use entity::review_findings::{FindingSeverity, FindingState};
use entity::{
    notification_settings, notifications, projects, review_findings, reviews, task_watchers,
    tenants, users,
};

// 定数本体は common へ移動（DTO からも参照するため）。既存の参照パス互換用に再公開。
pub use common::notifications::{
    DEFAULT_IN_APP_EVENTS, KNOWN_EVENT_TYPES, TYPE_ASSIGNED, TYPE_COMMENT_ADDED, TYPE_MENTIONED,
    TYPE_REVIEW_FINDING_CHANGED, TYPE_REVIEW_ROUND_ANY, TYPE_REVIEW_ROUND_CREATED,
    TYPE_STATUS_CHANGED,
};

pub async fn ensure_watcher<C: ConnectionTrait>(
    db: &C,
    task_id: Uuid,
    user_id: Uuid,
) -> Result<(), AppError> {
    task_watchers::Entity::insert(task_watchers::ActiveModel {
        task_id: Set(task_id),
        user_id: Set(user_id),
        created_at: Set(chrono::Utc::now().into()),
    })
    .on_conflict(
        OnConflict::columns([task_watchers::Column::TaskId, task_watchers::Column::UserId])
            .do_nothing()
            .to_owned(),
    )
    .exec_without_returning(db)
    .await?;
    Ok(())
}

async fn in_app_enabled<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    project_id: Uuid,
    event_type: &str,
) -> Result<bool, AppError> {
    let settings = notification_settings::Entity::find()
        .filter(notification_settings::Column::UserId.eq(user_id))
        .filter(notification_settings::Column::ProjectId.eq(project_id))
        .one(db)
        .await?;

    let events: Vec<String> = match settings {
        Some(s) => s.in_app_events,
        None => DEFAULT_IN_APP_EVENTS
            .iter()
            .map(|e| (*e).to_string())
            .collect(),
    };
    Ok(events.iter().any(|e| e == event_type))
}

/// `project_id` は通知の可視性の判定に使う（読み取り API が「入れないプロジェクトの
/// 通知」を落とす）。タスクに紐づかないレビュー通知も判定できるよう、`task_id` とは別に持つ。
pub async fn create_notification<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    project_id: Option<Uuid>,
    task_id: Option<Uuid>,
    notification_type: &str,
    payload: Json,
) -> Result<(), AppError> {
    notifications::ActiveModel {
        id: Set(Uuid::new_v4()),
        user_id: Set(user_id),
        task_id: Set(task_id),
        project_id: Set(project_id),
        notification_type: Set(notification_type.to_string()),
        payload: Set(payload),
        read_at: Set(None),
        created_at: Set(chrono::Utc::now().into()),
    }
    .insert(db)
    .await?;
    Ok(())
}

async fn notify_user_if_enabled<C: ConnectionTrait>(
    db: &C,
    user_id: Uuid,
    project_id: Uuid,
    task_id: Uuid,
    notification_type: &str,
    payload: Json,
) -> Result<(), AppError> {
    if !in_app_enabled(db, user_id, project_id, notification_type).await? {
        return Ok(());
    }
    create_notification(
        db,
        user_id,
        Some(project_id),
        Some(task_id),
        notification_type,
        payload,
    )
    .await
}

pub async fn notify_assigned<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    assignee_id: Uuid,
    assigned_by: Uuid,
    role: &str,
) -> Result<(), AppError> {
    ensure_watcher(db, task_id, assignee_id).await?;
    let assigner_name = users::Entity::find_by_id(assigned_by)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    notify_user_if_enabled(
        db,
        assignee_id,
        project_id,
        task_id,
        TYPE_ASSIGNED,
        serde_json::json!({
            "assigned_by": assigner_name,
            "role": role,
        }),
    )
    .await
}

/// そのプロジェクトの通知を受け取ってよい利用者。
///
/// プロジェクトに入れないユーザーには通知しない。メンバー未指定のプロジェクトは
/// テナントメンバー全員が宛先になる（#568）。テナントオーナーは `project_members` に
/// 入っていなくても受け取る。
async fn notifiable_user_ids<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let mut ids: HashSet<Uuid> = crate::access::project_accessible_user_ids(db, project_id).await?;

    let tenant_id = projects::Entity::find_by_id(project_id)
        .one(db)
        .await?
        .map(|p| p.tenant_id);
    if let Some(tenant_id) = tenant_id
        && let Some(tenant) = tenants::Entity::find_by_id(tenant_id).one(db).await?
    {
        ids.insert(tenant.owner_id);
    }
    Ok(ids)
}

pub async fn notify_watchers<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    notification_type: &str,
    payload: Json,
    exclude: &[Uuid],
) -> Result<(), AppError> {
    let exclude_set: HashSet<Uuid> = exclude.iter().copied().collect();
    let notifiable = notifiable_user_ids(db, project_id).await?;

    let watchers = task_watchers::Entity::find()
        .filter(task_watchers::Column::TaskId.eq(task_id))
        .all(db)
        .await?;
    for watcher in watchers {
        if exclude_set.contains(&watcher.user_id) || !notifiable.contains(&watcher.user_id) {
            continue;
        }
        notify_user_if_enabled(
            db,
            watcher.user_id,
            project_id,
            task_id,
            notification_type,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}

/// メンション通知を送信し、実際に通知が届いたユーザーのIDリストを返す。
/// 返値は notify_comment_added の除外リストに使うこと。
pub async fn notify_mentioned<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    mentioned_user_ids: &[Uuid],
    comment_id: Uuid,
    author_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    let author_name = users::Entity::find_by_id(author_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    let payload: Json = serde_json::json!({
        "comment_id": comment_id,
        "author": author_name,
    });
    let mut notified = Vec::new();
    for user_id in mentioned_user_ids {
        if *user_id == author_id {
            continue;
        }
        // メンション = 関心あり → notify_assigned と同様にウォッチャーへ自動追加
        ensure_watcher(db, task_id, *user_id).await?;
        if in_app_enabled(db, *user_id, project_id, TYPE_MENTIONED).await? {
            create_notification(
                db,
                *user_id,
                Some(project_id),
                Some(task_id),
                TYPE_MENTIONED,
                payload.clone(),
            )
            .await?;
            notified.push(*user_id);
        }
    }
    Ok(notified)
}

pub async fn notify_comment_added<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    comment_id: Uuid,
    author_id: Uuid,
    mentioned_user_ids: &[Uuid],
) -> Result<(), AppError> {
    let author_name = users::Entity::find_by_id(author_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    let payload: Json = serde_json::json!({
        "comment_id": comment_id,
        "author": author_name,
    });
    let mut exclude = vec![author_id];
    exclude.extend_from_slice(mentioned_user_ids);
    notify_watchers(
        db,
        project_id,
        task_id,
        TYPE_COMMENT_ADDED,
        payload,
        &exclude,
    )
    .await
}

pub async fn notify_status_changed<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    actor_id: Uuid,
    from: &str,
    to: &str,
) -> Result<(), AppError> {
    let actor_name = users::Entity::find_by_id(actor_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into());
    notify_watchers(
        db,
        project_id,
        task_id,
        TYPE_STATUS_CHANGED,
        serde_json::json!({
            "from": from,
            "to": to,
            "changed_by": actor_name,
        }),
        &[actor_id],
    )
    .await
}

/// 表示名。消えた利用者を参照している通知でも本題（何が起きたか）は出す。
async fn username<C: ConnectionTrait>(db: &C, user_id: Uuid) -> Result<String, AppError> {
    Ok(users::Entity::find_by_id(user_id)
        .one(db)
        .await?
        .map(|u| u.username)
        .unwrap_or_else(|| "unknown".into()))
}

/// ラウンドが見ていたリポジトリ（`owner/name`）。連携の無いプロジェクトでは空文字。
fn repo_label(review: &reviews::Model) -> String {
    if crate::reviews::RepoRef::of_round(review).is_linked() {
        format!("{}/{}", review.repo_owner, review.repo_name)
    } else {
        String::new()
    }
}

/// `review_round_any` を明示的に入れている購読者。
///
/// 設定で ON にした人なので、`in_app_enabled` は重ねて見ない
/// （`review_round_any` 自体は通知種別ではなく受信者の印）。
async fn round_subscribers<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    Ok(notification_settings::Entity::find()
        .filter(notification_settings::Column::ProjectId.eq(project_id))
        .all(db)
        .await?
        .into_iter()
        .filter(|row| row.in_app_events.iter().any(|e| e == TYPE_REVIEW_ROUND_ANY))
        .map(|row| row.user_id)
        .collect())
}

fn round_created_payload(
    review: &reviews::Model,
    findings: &[review_findings::Model],
    reviewer: &str,
) -> Json {
    let count = |severity: FindingSeverity| {
        findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    };
    serde_json::json!({
        "project_id": review.project_id,
        "review_id": review.id,
        "repo": repo_label(review),
        "pr_number": review.pr_number,
        "round": review.round,
        "head_sha": review.head_sha,
        "reviewer": reviewer,
        "counts": {
            "high": count(FindingSeverity::High),
            "medium": count(FindingSeverity::Medium),
            "low": count(FindingSeverity::Low),
            "nit": count(FindingSeverity::Nit),
        },
        // 総評は長文になりうる。一覧に出すぶんだけ切る（char 境界で切って壊さない）
        "summary_excerpt": review.summary.chars().take(200).collect::<String>(),
    })
}

/// ラウンドの起票を関係者と購読者へ知らせる。
///
/// 宛先は「その PR の関係者（[`crate::reviews::review_participants`]）」と
/// 「`review_round_any` を入れた購読者」の和。起票した本人と、そのプロジェクトに
/// 入れない利用者は落とす。
pub async fn notify_review_round_created<C: ConnectionTrait>(
    db: &C,
    review: &reviews::Model,
    findings: &[review_findings::Model],
    actor_id: Uuid,
) -> Result<(), AppError> {
    let participants = crate::reviews::review_participants(db, review).await?;
    let subscribers = round_subscribers(db, review.project_id).await?;
    let notifiable = notifiable_user_ids(db, review.project_id).await?;
    let payload = round_created_payload(review, findings, &username(db, review.reviewer_id).await?);

    for user_id in participants.union(&subscribers).copied() {
        if user_id == actor_id || !notifiable.contains(&user_id) {
            continue;
        }
        if !subscribers.contains(&user_id)
            && !in_app_enabled(db, user_id, review.project_id, TYPE_REVIEW_ROUND_CREATED).await?
        {
            continue;
        }
        create_notification(
            db,
            user_id,
            Some(review.project_id),
            None,
            TYPE_REVIEW_ROUND_CREATED,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}

/// ラウンドの起票を 1 人へ知らせる。
///
/// 要約ジョブが後から PR の作者を解決したとき（起票時点では作者が分からない）に使う。
/// 購読者は [`notify_review_round_created`] が拾うので、ここでは広げない。
pub async fn notify_review_round_created_to<C: ConnectionTrait>(
    db: &C,
    review: &reviews::Model,
    findings: &[review_findings::Model],
    user_id: Uuid,
) -> Result<(), AppError> {
    if !notifiable_user_ids(db, review.project_id)
        .await?
        .contains(&user_id)
        || !in_app_enabled(db, user_id, review.project_id, TYPE_REVIEW_ROUND_CREATED).await?
    {
        return Ok(());
    }
    // 過去ラウンドから作者が解決できた場合や購読者の場合は、起票時に通知済み。
    if notifications::Entity::find()
        .filter(notifications::Column::UserId.eq(user_id))
        .filter(notifications::Column::NotificationType.eq(TYPE_REVIEW_ROUND_CREATED))
        .filter(Expr::cust_with_values(
            "payload ->> 'review_id' = $1",
            [review.id.to_string()],
        ))
        .one(db)
        .await?
        .is_some()
    {
        return Ok(());
    }
    let payload = round_created_payload(review, findings, &username(db, review.reviewer_id).await?);
    create_notification(
        db,
        user_id,
        Some(review.project_id),
        None,
        TYPE_REVIEW_ROUND_CREATED,
        payload,
    )
    .await
}

/// 指摘の状態遷移を関係者へ知らせる。
///
/// 購読の印（`review_round_any`）はラウンドの起票にだけ効く。遷移まで配ると、
/// PR を見ていない人へ指摘 1 件ごとの通知が流れる。
pub async fn notify_review_finding_changed<C: ConnectionTrait>(
    db: &C,
    review: &reviews::Model,
    finding: &review_findings::Model,
    from: FindingState,
    to: FindingState,
    actor_id: Uuid,
    note: Option<&str>,
) -> Result<(), AppError> {
    let notifiable = notifiable_user_ids(db, review.project_id).await?;
    let payload: Json = serde_json::json!({
        "project_id": review.project_id,
        "review_id": review.id,
        "finding_id": finding.id,
        "repo": repo_label(review),
        "pr_number": review.pr_number,
        "round": review.round,
        "title": finding.title,
        "severity": finding.severity.as_str(),
        "from": from.as_str(),
        "to": to.as_str(),
        "actor": username(db, actor_id).await?,
        "note": note,
    });

    for user_id in crate::reviews::review_participants(db, review).await? {
        if user_id == actor_id || !notifiable.contains(&user_id) {
            continue;
        }
        if !in_app_enabled(db, user_id, review.project_id, TYPE_REVIEW_FINDING_CHANGED).await? {
            continue;
        }
        create_notification(
            db,
            user_id,
            Some(review.project_id),
            None,
            TYPE_REVIEW_FINDING_CHANGED,
            payload.clone(),
        )
        .await?;
    }
    Ok(())
}
