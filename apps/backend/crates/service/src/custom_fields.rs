use crate::error::AppError;
// DTO 型（CustomFieldDefinitionSummary 等）は payload 側へ移動済み。ここはロジックのみ。
use chrono::NaiveDate;
use entity::{project_custom_fields, task_custom_field_values};
use payload::custom_fields::{
    CustomFieldDefinitionSummary, CustomFieldValueInput, TaskCustomFieldValueResponse,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, prelude::Uuid,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub fn validate_select_options(options: &Option<Value>) -> Result<(), AppError> {
    let Some(options) = options else {
        return Err(AppError::BadRequest);
    };
    let arr = options.as_array().ok_or(AppError::BadRequest)?;
    if arr.is_empty() {
        return Err(AppError::BadRequest);
    }
    let mut seen_values = HashSet::new();
    for item in arr {
        let obj = item.as_object().ok_or(AppError::BadRequest)?;
        let label = obj.get("label").and_then(|v| v.as_str());
        let value = obj.get("value").and_then(|v| v.as_str());
        if label.map(|s| s.trim().is_empty()).unwrap_or(true)
            || value.map(|s| s.trim().is_empty()).unwrap_or(true)
        {
            return Err(AppError::BadRequest);
        }
        // value に前後の空白があると lookup 時に不一致が起きるため拒否
        if value.map(|s| s != s.trim()).unwrap_or(false) {
            return Err(AppError::BadRequest);
        }
        if !seen_values.insert(value.unwrap()) {
            return Err(AppError::BadRequest);
        }
    }
    Ok(())
}

pub fn validate_custom_field_value(
    field: &project_custom_fields::Model,
    value: &str,
) -> Result<(), AppError> {
    if value.is_empty() {
        return Err(AppError::BadRequest);
    }
    match field.field_type {
        project_custom_fields::CustomFieldType::Text => Ok(()),
        project_custom_fields::CustomFieldType::Number => {
            let n = value.parse::<f64>().map_err(|_| AppError::BadRequest)?;
            if n.is_finite() {
                Ok(())
            } else {
                Err(AppError::BadRequest)
            }
        }
        project_custom_fields::CustomFieldType::Select => {
            let options = field.options.as_ref().ok_or(AppError::BadRequest)?;
            let arr = options.as_array().ok_or(AppError::BadRequest)?;
            let allowed: Vec<&str> = arr
                .iter()
                .filter_map(|o| o.get("value").and_then(|v| v.as_str()))
                .collect();
            if allowed.contains(&value) {
                Ok(())
            } else {
                Err(AppError::BadRequest)
            }
        }
        project_custom_fields::CustomFieldType::Date => {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map(|_| ())
                .map_err(|_| AppError::BadRequest)
        }
        project_custom_fields::CustomFieldType::Url => {
            let parsed = url::Url::parse(value).map_err(|_| AppError::BadRequest)?;
            if matches!(parsed.scheme(), "http" | "https") {
                Ok(())
            } else {
                Err(AppError::BadRequest)
            }
        }
        project_custom_fields::CustomFieldType::Checkbox => {
            if value == "true" || value == "false" {
                Ok(())
            } else {
                Err(AppError::BadRequest)
            }
        }
    }
}

pub fn display_value_for(field: &project_custom_fields::Model, value: &str) -> String {
    if field.field_type != project_custom_fields::CustomFieldType::Select {
        return value.to_string();
    }
    let Some(options) = field.options.as_ref() else {
        return value.to_string();
    };
    let Some(arr) = options.as_array() else {
        return value.to_string();
    };
    for item in arr {
        if item.get("value").and_then(|v| v.as_str()) == Some(value) {
            return item
                .get("label")
                .and_then(|l| l.as_str())
                .unwrap_or(value)
                .to_string();
        }
    }
    value.to_string()
}

pub async fn load_task_custom_field_values<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
) -> Result<Vec<TaskCustomFieldValueResponse>, AppError> {
    let fields = project_custom_fields::Entity::find()
        .filter(project_custom_fields::Column::ProjectId.eq(project_id))
        .order_by_asc(project_custom_fields::Column::Position)
        .order_by_asc(project_custom_fields::Column::CreatedAt)
        .all(db)
        .await?;
    let values = task_custom_field_values::Entity::find()
        .filter(task_custom_field_values::Column::TaskId.eq(task_id))
        .all(db)
        .await?;
    let value_map: HashMap<Uuid, String> = values
        .into_iter()
        .filter_map(|v| v.value.map(|val| (v.field_id, val)))
        .collect();
    Ok(fields
        .into_iter()
        .map(|field| {
            let value = value_map.get(&field.id).cloned();
            let display_value = value.as_deref().map(|v| display_value_for(&field, v));
            TaskCustomFieldValueResponse {
                field: CustomFieldDefinitionSummary::from(&field),
                value,
                display_value,
            }
        })
        .collect())
}

/// カスタムフィールド値を upsert する。
/// **必ずトランザクション内で呼ぶこと。** 複数行を個別に INSERT/UPDATE/DELETE するため、
/// トランザクション外で呼ぶと並行リクエストで中途半端な状態が残るリスクがある。
pub async fn upsert_task_custom_field_values<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    inputs: &[CustomFieldValueInput],
) -> Result<(), AppError> {
    if inputs.is_empty() {
        return Ok(());
    }
    let field_ids: Vec<Uuid> = inputs.iter().map(|i| i.field_id).collect();
    let unique_ids: HashSet<Uuid> = field_ids.iter().copied().collect();
    if unique_ids.len() != field_ids.len() {
        return Err(AppError::BadRequest);
    }
    let fields = project_custom_fields::Entity::find()
        .filter(project_custom_fields::Column::ProjectId.eq(project_id))
        .filter(project_custom_fields::Column::Id.is_in(field_ids))
        .all(db)
        .await?;
    if fields.len() != unique_ids.len() {
        return Err(AppError::BadRequest);
    }
    let field_map: HashMap<Uuid, project_custom_fields::Model> =
        fields.into_iter().map(|f| (f.id, f)).collect();
    for input in inputs {
        let field = field_map.get(&input.field_id).ok_or(AppError::BadRequest)?;
        match input.value.as_deref() {
            None | Some("") => {
                if field.is_required {
                    return Err(AppError::BadRequest);
                }
                task_custom_field_values::Entity::delete_many()
                    .filter(task_custom_field_values::Column::TaskId.eq(task_id))
                    .filter(task_custom_field_values::Column::FieldId.eq(input.field_id))
                    .exec(db)
                    .await?;
            }
            Some(value) => {
                validate_custom_field_value(field, value)?;
                let existing = task_custom_field_values::Entity::find()
                    .filter(task_custom_field_values::Column::TaskId.eq(task_id))
                    .filter(task_custom_field_values::Column::FieldId.eq(input.field_id))
                    .one(db)
                    .await?;
                if let Some(row) = existing {
                    let mut active: task_custom_field_values::ActiveModel = row.into();
                    active.value = Set(Some(value.to_string()));
                    active.update(db).await?;
                } else {
                    task_custom_field_values::ActiveModel {
                        task_id: Set(task_id),
                        field_id: Set(input.field_id),
                        value: Set(Some(value.to_string())),
                    }
                    .insert(db)
                    .await?;
                }
            }
        }
    }
    Ok(())
}

/// 必須カスタムフィールドが全て埋まっているか検証する。
/// `pending` に upsert 予定の値を渡すことで、DB への保存前でも正しく検証できる。
/// 新規作成時は DB 上に値がないため、`pending` で全値を渡す必要がある。
pub async fn ensure_required_custom_fields<C: ConnectionTrait>(
    db: &C,
    project_id: Uuid,
    task_id: Uuid,
    pending: Option<&[CustomFieldValueInput]>,
) -> Result<(), AppError> {
    let required_fields = project_custom_fields::Entity::find()
        .filter(project_custom_fields::Column::ProjectId.eq(project_id))
        .filter(project_custom_fields::Column::IsRequired.eq(true))
        .all(db)
        .await?;
    if required_fields.is_empty() {
        return Ok(());
    }
    let existing = task_custom_field_values::Entity::find()
        .filter(task_custom_field_values::Column::TaskId.eq(task_id))
        .all(db)
        .await?;
    let mut value_map: HashMap<Uuid, Option<String>> = existing
        .into_iter()
        .map(|v| (v.field_id, v.value))
        .collect();
    if let Some(pending_values) = pending {
        for input in pending_values {
            value_map.insert(input.field_id, input.value.clone());
        }
    }
    for field in required_fields {
        let value = value_map.get(&field.id);
        let valid = matches!(value, Some(Some(v)) if !v.is_empty());
        if !valid {
            return Err(AppError::BadRequest);
        }
    }
    Ok(())
}
