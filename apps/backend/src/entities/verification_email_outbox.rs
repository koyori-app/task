use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(20))")]
pub enum OutboxStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    /// ワーカーが送信中（他ワーカーとの二重処理防止）
    #[sea_orm(string_value = "processing")]
    Processing,
    #[sea_orm(string_value = "sent")]
    Sent,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "verification_email_outbox")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(indexed)]
    pub user_id: Uuid,
    pub email: String,
    /// 送信完了後はクリアする（平文トークンを永続保持しない）
    #[sea_orm(nullable)]
    pub token: Option<String>,
    #[sea_orm(indexed)]
    pub status: OutboxStatus,
    pub attempts: i32,
    #[sea_orm(nullable)]
    pub last_error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    /// ワーカーが processing にした時刻（クラッシュ後の再取得用）
    #[sea_orm(nullable)]
    pub claimed_at: Option<DateTimeWithTimeZone>,
    #[sea_orm(nullable)]
    pub sent_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Users,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Users.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
