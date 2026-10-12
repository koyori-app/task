//! ホスト上のログイン名を Task の利用者へ解決する。
//!
//! 由来によって使ってよい範囲が違う:
//! - コミットの作者（`author_handle`）はホストがコミットのメールアドレスから解決した値で、
//!   署名の無いコミットなら偽装できる。**表示にだけ**使い、通知や権限の根拠にしない
//! - PR の作者はホストの API が返す login なので、**通知の宛先**に使ってよい

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, prelude::Uuid};

use entity::oauth_connections;

/// ホスト上のログイン名（小文字）を控えた接続がちょうど 1 件のときだけ、その利用者を返す。
///
/// ログイン名の一意性は保存側の付け替えで保っているので、競合で 2 件あっても決め打ちしない。
pub async fn user_id_for_login<C: ConnectionTrait>(
    db: &C,
    provider: &str,
    login: &str,
) -> Result<Option<Uuid>, sea_orm::DbErr> {
    if login.is_empty() {
        return Ok(None);
    }
    // ponytail: クラウド版（instance_url が NULL）の接続だけを見る。セルフホストの GitLab / Forgejo を足すときは host_url と instance_url を突き合わせる
    let user_ids: Vec<Uuid> = oauth_connections::Entity::find()
        .filter(oauth_connections::Column::Provider.eq(provider))
        .filter(oauth_connections::Column::InstanceUrl.is_null())
        .filter(oauth_connections::Column::ProviderLogin.eq(login.to_lowercase()))
        .select_only()
        .column(oauth_connections::Column::UserId)
        .limit(2)
        .into_tuple()
        .all(db)
        .await?;
    Ok(match user_ids.as_slice() {
        [user_id] => Some(*user_id),
        _ => None,
    })
}
