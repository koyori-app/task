use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
pub enum CustomFieldType {
    #[sea_orm(string_value = "text")]
    Text,
    #[sea_orm(string_value = "number")]
    Number,
    #[sea_orm(string_value = "select")]
    Select,
    #[sea_orm(string_value = "date")]
    Date,
    #[sea_orm(string_value = "url")]
    Url,
    #[sea_orm(string_value = "checkbox")]
    Checkbox,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "project_custom_fields")]
#[schema(as = crate::entities::project_custom_fields::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub id: Uuid,
    #[schema(value_type = String, format = "uuid")]
    pub project_id: Uuid,
    pub name: String,
    pub field_type: CustomFieldType,
    #[sea_orm(column_type = "JsonBinary", nullable)]
    #[schema(nullable)]
    pub options: Option<Json>,
    pub is_required: bool,
    pub position: i16,
    #[schema(value_type = String, format = "date-time")]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::projects::Entity",
        from = "Column::ProjectId",
        to = "super::projects::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Projects,
}

impl Related<super::projects::Entity> for Entity {
    fn to() -> RelationDef { Relation::Projects.def() }
}

impl ActiveModelBehavior for ActiveModel {}
