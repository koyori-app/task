//! OpenAPI コンポーネント登録。

pub mod responses;

use utoipa::openapi::OpenApi;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::openapi::tag::TagBuilder;
use utoipa::{PartialSchema, ToSchema};

pub use crate::error::ServerError;
pub use responses::{
    CredentialErrors, CrudErrors, DesktopAuthTokenErrors, DriveFolderErrors, InternalOnlyError,
    OAuthErrors, PasswordChangeErrors, PasswordResetCompleteErrors, PasswordResetRequestErrors,
    PasswordResetVerifyErrors, PersonalTokenAuthErrors, PublicShareErrors, RegisterErrors,
    ResendVerificationErrors, SessionAuthErrors, TenantCreateErrors, UnauthorizedErrors,
    VerifyEmailErrors,
};

/// スキーマのうち、ハンドラだけでは OpenAPI に載らないものを登録する。
pub fn register_schemas(openapi: &mut OpenApi) {
    let components = openapi
        .components
        .get_or_insert_with(utoipa::openapi::Components::new);

    register_schema::<ServerError>(components);
    register_schema::<payload::auth_2fa::Login2faResponse>(components);
    register_schema::<payload::auth_2fa::TotpSetupResponse>(components);
    register_schema::<payload::auth_2fa::VerifySetupResponse>(components);
    register_security_schemes(components);
    register_tags(openapi);
}

/// Scalar のグループ表示順とタグ説明を定義する。
fn register_tags(openapi: &mut OpenApi) {
    openapi.tags = Some(vec![
        TagBuilder::new()
            .name("Auth")
            .description(Some("認証・セッション・パスキー管理"))
            .build(),
        TagBuilder::new()
            .name("Tenants")
            .description(Some("テナント管理"))
            .build(),
        TagBuilder::new()
            .name("Projects")
            .description(Some("プロジェクト管理"))
            .build(),
        TagBuilder::new()
            .name("Project Members")
            .description(Some("プロジェクトメンバー管理"))
            .build(),
        TagBuilder::new()
            .name("Labels")
            .description(Some("ラベル一覧"))
            .build(),
        TagBuilder::new()
            .name("Custom Fields")
            .description(Some("プロジェクトカスタムフィールド定義"))
            .build(),
        TagBuilder::new()
            .name("Personal Tokens")
            .description(Some("パーソナルアクセストークン（PAT）管理"))
            .build(),
        TagBuilder::new()
            .name("Drive Files")
            .description(Some("ドライブ — ファイル操作"))
            .build(),
        TagBuilder::new()
            .name("Drive Folders")
            .description(Some("ドライブ — フォルダ管理"))
            .build(),
        TagBuilder::new()
            .name("Drive Shares")
            .description(Some("ドライブ — フォルダ共有・公開リンク"))
            .build(),
        TagBuilder::new()
            .name("Admin Users")
            .description(Some("管理者 — ユーザー管理"))
            .build(),
        TagBuilder::new()
            .name("Admin Tenants")
            .description(Some("管理者 — テナント閲覧・削除"))
            .build(),
        TagBuilder::new()
            .name("Admin Audit Logs")
            .description(Some("管理者 — 監査ログ一覧"))
            .build(),
        TagBuilder::new()
            .name("Admin System Settings")
            .description(Some("管理者 — システム設定"))
            .build(),
        TagBuilder::new()
            .name("GitHub")
            .description(Some("GitHub App 連携・Webhook"))
            .build(),
        TagBuilder::new()
            .name("My Tasks")
            .description(Some("個人todo管理 — My Tasks ビュー・クイックキャプチャ"))
            .build(),
        TagBuilder::new()
            .name("Schedule")
            .description(Some(
                "期間に重なるタスク・スプリント・マイルストーン（カレンダー・タイムライン）",
            ))
            .build(),
        TagBuilder::new()
            .name("Notifications")
            .description(Some("in-app 通知・ウォッチャー・通知設定"))
            .build(),
        TagBuilder::new()
            .name("Desktop")
            .description(Some("Koyori Desktop の認証（Device Token）・端末管理"))
            .build(),
    ]);
}

/// PAT 等で利用する Bearer 認証スキーム（`Authorization: Bearer <token>`）。
fn register_security_schemes(components: &mut utoipa::openapi::Components) {
    components.security_schemes.insert(
        "bearerAuth".to_string(),
        SecurityScheme::Http(
            HttpBuilder::new()
                .scheme(HttpAuthScheme::Bearer)
                .bearer_format("PAT")
                .build(),
        ),
    );
}

fn register_schema<T>(components: &mut utoipa::openapi::Components)
where
    T: ToSchema + PartialSchema,
{
    let name = T::name().into_owned();
    components.schemas.entry(name).or_insert_with(T::schema);
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use payload::or_unknown::OR_UNKNOWN_EXTENSION;
    use serde_json::Value;
    use utoipa_axum::router::OpenApiRouter;

    /// 応答の列挙欄のうち `OrUnknown` で包まないもの（スキーマ名, 欄名）。
    ///
    /// どれも DTO が `Deserialize` を持たず、CLI が読めない（CLI は payload の型で読む）。
    /// 画面はサーバーと同時に配るので、知らない値を受けることがない。
    /// DTO に `Deserialize` を付けたら、ここから外して欄を `OrUnknown` で包む。
    const UNWRAPPED_RESPONSE_ENUMS: &[(&str, &str)] = &[
        ("CreatePersonalTokenResponse", "scopes"),
        ("DriveFolderShareResponse", "permission"),
        ("ExplicitProjectAce", "role"),
        ("InvitationPreviewResponse", "role"),
        ("PersonalTokenResponse", "scopes"),
        ("ProjectCustomFieldResponse", "field_type"),
        ("TenantInvitationResponse", "role"),
        ("TenantListItemResponse", "member_role"),
        ("TenantListItemResponse", "membership"),
        ("TenantMemberResponse", "role"),
    ];

    /// `$ref` を、`$ref` でないスキーマに着くまで辿る（別名の別名も辿る）。
    ///
    /// 返すのは最後に辿った先の名前（辿らなければ `None`）と中身、途中も含めてどこかに
    /// 目印があったか。`$ref` だけの循環は、同じ名前に戻ったところで止める。
    fn resolve<'a>(doc: &'a Value, schema: &'a Value) -> (Option<&'a str>, &'a Value, bool) {
        let mut chain = BTreeSet::new();
        let (mut name, mut current) = (None, schema);
        let mut marked = false;
        loop {
            marked |= current.get(OR_UNKNOWN_EXTENSION).is_some();
            let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
                break;
            };
            let next = reference
                .strip_prefix("#/components/schemas/")
                .unwrap_or_else(|| panic!("components 以外を指す $ref: {reference}"));
            if !chain.insert(next) {
                break;
            }
            current = &doc["components"]["schemas"][next];
            assert!(
                !current.is_null(),
                "未登録のスキーマを指す $ref: {reference}"
            );
            name = Some(next);
        }
        (name, current, marked)
    }

    fn children(schema: &Value) -> impl Iterator<Item = &Value> {
        let composed = ["allOf", "oneOf", "anyOf"]
            .into_iter()
            .filter_map(|key| schema.get(key).and_then(Value::as_array))
            .flatten();
        let nested = ["items", "additionalProperties"]
            .into_iter()
            .filter_map(|key| schema.get(key))
            .filter(|child| child.is_object());
        composed.chain(nested)
    }

    /// 目印を通らずに string の列挙へ届くか。オブジェクトの中は別のスキーマとして歩くので入らない。
    /// `visited` は辿った名前付きスキーマ。再帰する型で同じスキーマへ戻ったら打ち切る。
    fn reaches_bare_enum<'a>(
        doc: &'a Value,
        schema: &'a Value,
        visited: &mut BTreeSet<&'a str>,
    ) -> bool {
        let (name, schema, marked) = resolve(doc, schema);
        if marked || name.is_some_and(|name| !visited.insert(name)) {
            return false;
        }
        let is_string = match &schema["type"] {
            Value::String(ty) => ty == "string",
            Value::Array(types) => types.iter().any(|ty| ty == "string"),
            _ => false,
        };
        if schema.get("enum").is_some() && is_string {
            return true;
        }
        schema.get("properties").is_none()
            && children(schema).any(|child| reaches_bare_enum(doc, child, visited))
    }

    /// 応答から辿れるスキーマを歩き、素の列挙を型に持つ欄を `スキーマ名.欄名` で集める。
    fn collect_bare_enum_fields<'a>(
        doc: &'a Value,
        schema: &'a Value,
        owner: &str,
        seen: &mut BTreeSet<&'a str>,
        found: &mut BTreeSet<String>,
    ) {
        let (name, schema, _) = resolve(doc, schema);
        if let Some(name) = name
            && !seen.insert(name)
        {
            return;
        }
        let owner = name.unwrap_or(owner).to_string();
        for (field, property) in schema
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            if reaches_bare_enum(doc, property, &mut BTreeSet::new()) {
                found.insert(format!("{owner}.{field}"));
            }
            collect_bare_enum_fields(doc, property, &owner, seen, found);
        }
        for child in children(schema) {
            collect_bare_enum_fields(doc, child, &owner, seen, found);
        }
    }

    fn bare_enum_response_fields(doc: &Value) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut found = BTreeSet::new();
        for (path, item) in doc["paths"].as_object().expect("paths object") {
            for (method, operation) in item.as_object().expect("path item object") {
                for (status, response) in operation
                    .get("responses")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    for media in response
                        .get("content")
                        .and_then(Value::as_object)
                        .into_iter()
                        .flat_map(|content| content.values())
                    {
                        if let Some(schema) = media.get("schema") {
                            let owner = format!("{} {path} {status}", method.to_uppercase());
                            collect_bare_enum_fields(doc, schema, &owner, &mut seen, &mut found);
                        }
                    }
                }
            }
        }
        found
    }

    /// 応答の列挙欄は `OrUnknown<T>` で受ける。素の列挙だと、サーバーが値を足したとき
    /// 配布済みの CLI が応答を丸ごと読めなくなる。リクエスト本文は歩かない（CLI が送る側）。
    #[test]
    fn response_enum_fields_are_wrapped_in_or_unknown() {
        let (_, mut openapi) = OpenApiRouter::<crate::AppState>::new()
            .merge(crate::routes::create_routes())
            .split_for_parts();
        super::register_schemas(&mut openapi);
        let doc = serde_json::to_value(&openapi).expect("serialize openapi");

        let found = bare_enum_response_fields(&doc);
        let allowed: BTreeSet<String> = UNWRAPPED_RESPONSE_ENUMS
            .iter()
            .map(|(schema, field)| format!("{schema}.{field}"))
            .collect();

        let unwrapped: Vec<&String> = found.difference(&allowed).collect();
        assert!(
            unwrapped.is_empty(),
            "応答の列挙欄が素の列挙型です。CLI が読む DTO の欄は `payload::or_unknown::OrUnknown<T>` で包み、\
             `#[schema(inline)]` を付けてください（CLI が読まない DTO なら UNWRAPPED_RESPONSE_ENUMS に足す）: {unwrapped:#?}"
        );
        let stale: Vec<&String> = allowed.difference(&found).collect();
        assert!(
            stale.is_empty(),
            "UNWRAPPED_RESPONSE_ENUMS に、もう素の列挙欄でない項目があります。外してください: {stale:#?}"
        );
    }
}
