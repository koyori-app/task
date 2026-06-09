use sea_orm::entity::prelude::*;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, ToSchema, serde::Serialize)]
#[sea_orm(table_name = "task_custom_field_values")]
#[schema(as = crate::entities::task_custom_field_values::Model)]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub task_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    #[schema(value_type = String, format = "uuid")]
    pub field_id: Uuid,
    #[sea_orm(nullable)]
    #[schema(nullable)]
    pub value: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::tasks::Entity",
        from = "Column::TaskId",
        to = "super::tasks::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    Tasks,
    #[sea_orm(
        belongs_to = "super::project_custom_fields::Entity",
        from = "Column::FieldId",
        to = "super::project_custom_fields::Column::Id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    ProjectCustomFields,
}

impl Related<super::tasks::Entity> for Entity {
    fn to() -> RelationDef { Relation::Tasks.def() }
}

impl Related<super::project_custom_fields::Entity> for Entity {
    fn to() -> RelationDef { Relation::ProjectCustomFields.def() }
}

impl ActiveModelBehavior for ActiveModel {}
