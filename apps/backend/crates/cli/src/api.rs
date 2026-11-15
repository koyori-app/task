//! API クライアント。応答は `payload` の型でそのまま受ける。
//!
//! 手書きの型を挟まないのは、フィールド名の食い違いを実行時ではなくコンパイル時に
//! 出したいため（#647 で手書き `paths.ts` が実際の応答とずれていた）。

use payload::personal_tokens::PersonalTokenTenants;
use payload::tenants::TenantRef;
use reqwest::{Method, StatusCode, Url};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::config::{RuntimeConfig, TenantSource};
use crate::error::{CliError, Result};

pub struct ApiClient {
    http: reqwest::Client,
    base: Url,
    tenant_id: String,
    /// 明示した tenant の由来。自動解決・tenant 無しの口では `None`
    tenant_source: Option<TenantSource>,
    /// テナントを名前（slug）で指したときの、その slug。ID に直した結果と並べて持つ
    tenant_slug: Option<String>,
}

impl ApiClient {
    pub fn new(config: RuntimeConfig) -> Result<Self> {
        let base = Url::parse(&config.api_url).map_err(|err| {
            CliError::validation(format!("Invalid api_url {}: {err}", config.api_url))
        })?;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", config.token).parse().map_err(|_| {
                CliError::validation("The configured token is not a valid header value")
            })?,
        );
        headers.insert(reqwest::header::ACCEPT, "application/json".parse().unwrap());
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .map_err(CliError::from)?;
        let (tenant_id, tenant_source) = config.tenant.unzip();
        Ok(Self {
            http,
            base,
            tenant_id: tenant_id.unwrap_or_default(),
            tenant_source,
            tenant_slug: None,
        })
    }

    /// 行き先のテナントを決める。
    ///
    /// - 明示が UUID: そのまま使う（何も叩かない）。
    /// - 明示が UUID でない: 名前（slug）として `GET /v1/tenants` から ID に直す。
    /// - 明示なし: トークンの束縛先を `/me` から引く。束縛が 1 つならそれを使う。複数なら、
    ///   黙って 1 つを選ぶと別のテナントへ書き込みかねないので、明示を求めて止まる（要求は送らない）。
    ///
    /// config には書かない。トークンを替えたときに古いテナントが残るため
    pub(crate) async fn resolve_tenant(mut self) -> Result<Self> {
        if self.tenant_source.is_some() {
            if crate::resolve::is_uuid(&self.tenant_id) {
                return Ok(self);
            }
            let name = self.tenant_id.clone();
            let tenants = self.list_tenants().await.map_err(|mut err| {
                if err.exit_code == 4 {
                    // 403 は束縛の食い違いではなく、名前を引く権限が無いこと。
                    // `http_error` が付ける「tenant_id is set by …」の案内は原因が違う
                    err.message = format!(
                        "Cannot look up the tenant \"{name}\" by name: the token needs the read:tenant scope. Use the tenant UUID instead."
                    );
                }
                err
            })?;
            let picked = pick_tenant(&tenants, &name)?;
            self.tenant_id = picked.id.to_string();
            self.tenant_slug = Some(picked.display_id.clone());
            return Ok(self);
        }
        let identity: PersonalTokenTenants =
            self.get(&["v1", "personal_tokens", "me"], &[]).await?;
        self.tenant_id = match identity.bound_tenant_ids().as_slice() {
            [only] => only.to_string(),
            [] => return Err(CliError::new("The token is not bound to any tenant")),
            many => {
                // 名前は補足。引けなくても（`read:tenant` が無い・通信の失敗）ID だけでこの
                // エラーを報告する。ここは既にエラーを返す途中なので、名前の取得の失敗は握り潰す
                let names = self.list_tenants().await.unwrap_or_default();
                let candidates = many
                    .iter()
                    .map(|id| match names.iter().find(|tenant| tenant.id == *id) {
                        Some(tenant) => format!("{} ({id})", tenant.display_id),
                        None => id.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CliError::validation(format!(
                    "This token can be used in {} tenants: {candidates}. Choose one with TASK_TENANT=<name or tenant_id>, or run `task config set tenant_id <name or tenant_id>` when api_url and token also come from the config.",
                    many.len()
                )));
            }
        };
        Ok(self)
    }

    /// このトークンで使えるテナント（`GET /v1/tenants`）。scope `read:tenant` が要る。
    pub(crate) async fn list_tenants(&self) -> Result<Vec<TenantRef>> {
        self.get(&["v1", "tenants"], &[]).await
    }

    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    pub async fn get<T: DeserializeOwned>(
        &self,
        segments: &[&str],
        query: &[(&str, String)],
    ) -> Result<T> {
        self.send(Method::GET, segments, query, None::<&()>).await
    }

    pub async fn post<B: Serialize, T: DeserializeOwned>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T> {
        self.send(Method::POST, segments, &[], Some(body)).await
    }

    pub async fn put<B: Serialize, T: DeserializeOwned>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T> {
        self.send(Method::PUT, segments, &[], Some(body)).await
    }

    pub async fn patch<B: Serialize, T: DeserializeOwned>(
        &self,
        segments: &[&str],
        body: &B,
    ) -> Result<T> {
        self.send(Method::PATCH, segments, &[], Some(body)).await
    }

    /// 本文を返さない削除。204 を空応答の失敗と取り違えないよう別扱いにする。
    pub async fn delete(&self, segments: &[&str]) -> Result<()> {
        self.send_no_content(Method::DELETE, segments).await
    }

    /// 本文を返さない更新（全件既読など）。
    pub async fn patch_no_content(&self, segments: &[&str]) -> Result<()> {
        self.send_no_content(Method::PATCH, segments).await
    }

    async fn send_no_content(&self, method: Method, segments: &[&str]) -> Result<()> {
        let url = self.url(segments, &[])?;
        let response = self.http.request(method, url).send().await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            Ok(())
        } else {
            Err(self.http_error(status, &body))
        }
    }

    async fn send<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        segments: &[&str],
        query: &[(&str, String)],
        body: Option<&B>,
    ) -> Result<T> {
        let url = self.url(segments, query)?;
        let path = url.path().to_string();
        let mut request = self.http.request(method, url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            return Err(self.http_error(status, &text));
        }
        if status == StatusCode::NO_CONTENT || text.trim().is_empty() {
            return Err(CliError::new("API returned empty response"));
        }
        serde_json::from_str(&text).map_err(|err| {
            // 応答の形が型と合わないのは、たいてい API 側の変更に追いついていない状態。
            // 握り潰すと「なぜか空」に見えるので、どこで何が合わないかを出す
            CliError::new(format!(
                "Cannot parse the API response for {path}: {err}. \
                 The CLI may be older than the server; update to the latest release."
            ))
        })
    }

    /// 明示 tenant で 403 を受けたら、tenant を外す案内を 1 行添える。
    /// 自動解決した tenant はトークンの束縛先そのものなので添えない
    fn http_error(&self, status: StatusCode, body: &str) -> CliError {
        let mut err = CliError::http(status.as_u16(), body);
        if let (StatusCode::FORBIDDEN, Some(source)) = (status, self.tenant_source) {
            err.message = format!("{}\n{}", err.message, source.forbidden_hint());
        }
        err
    }

    fn url(&self, segments: &[&str], query: &[(&str, String)]) -> Result<Url> {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| CliError::validation("api_url must be an http(s) URL"))?;
            // ベース URL がパス付き（`https://host/api`）でも繋がるよう、末尾の空要素を落とす
            path.pop_if_empty();
            path.extend(segments);
        }
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }
}

/// 名前（slug）に合うテナント。完全一致を優先し、無ければ大文字小文字を無視して照合する
/// （プロジェクトや状態の名前解決と同じ優先）。
fn pick_tenant<'a>(tenants: &'a [TenantRef], name: &str) -> Result<&'a TenantRef> {
    tenants
        .iter()
        .find(|tenant| tenant.display_id == name)
        .or_else(|| {
            tenants
                .iter()
                .find(|tenant| tenant.display_id.eq_ignore_ascii_case(name))
        })
        .ok_or_else(|| {
            let usable = if tenants.is_empty() {
                "none".to_string()
            } else {
                tenants
                    .iter()
                    .map(|tenant| format!("{} ({})", tenant.display_id, tenant.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            CliError::validation(format!(
                "Tenant not found: {name} (this token can be used in: {usable})"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(api_url: &str) -> ApiClient {
        ApiClient::new(RuntimeConfig {
            api_url: api_url.into(),
            token: "token-1".into(),
            tenant: Some(("tenant-1".into(), TenantSource::Config)),
        })
        .unwrap()
    }

    #[test]
    fn builds_urls_below_a_base_that_already_has_a_path() {
        let url = client("https://api.invalid/api")
            .url(&["v1", "auth", "me"], &[])
            .unwrap();
        assert_eq!(url.as_str(), "https://api.invalid/api/v1/auth/me");
    }

    #[test]
    fn percent_encodes_path_parameters_so_a_key_cannot_escape_its_segment() {
        let url = client("https://api.invalid")
            .url(
                &["v1", "tenants", "tenant-1", "projects", "../../admin"],
                &[],
            )
            .unwrap();
        assert_eq!(
            url.as_str(),
            "https://api.invalid/v1/tenants/tenant-1/projects/..%2F..%2Fadmin"
        );
    }

    #[test]
    fn appends_query_parameters_in_the_given_order() {
        let url = client("https://api.invalid")
            .url(
                &["v1", "reviews"],
                &[("pr", "618".into()), ("repo", "acme/old".into())],
            )
            .unwrap();
        assert_eq!(url.query(), Some("pr=618&repo=acme%2Fold"));
    }

    #[test]
    fn keeps_an_empty_repo_filter_distinguishable_from_an_absent_one() {
        // 空文字は「連携を張る前のラウンド」を指す。落とすと到達手段が無くなる
        let url = client("https://api.invalid")
            .url(&["v1", "reviews"], &[("repo", String::new())])
            .unwrap();
        assert_eq!(url.query(), Some("repo="));
    }

    fn tenant(id: u128, slug: &str) -> TenantRef {
        TenantRef {
            id: uuid::Uuid::from_u128(id),
            display_id: slug.into(),
            name: slug.to_uppercase(),
        }
    }

    #[test]
    fn picks_a_tenant_by_exact_name_before_a_case_insensitive_one() {
        let tenants = [tenant(1, "Acme"), tenant(2, "acme"), tenant(3, "dmm")];
        assert_eq!(pick_tenant(&tenants, "acme").unwrap().id.as_u128(), 2);
        assert_eq!(pick_tenant(&tenants, "ACME").unwrap().id.as_u128(), 1);
        assert_eq!(pick_tenant(&tenants, "dmm").unwrap().id.as_u128(), 3);
    }

    #[test]
    fn an_unknown_tenant_name_lists_the_usable_tenants() {
        let tenants = [tenant(1, "acme"), tenant(2, "dmm")];
        let err = pick_tenant(&tenants, "nope").unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert!(
            err.message.contains("Tenant not found: nope"),
            "{}",
            err.message
        );
        assert!(err.message.contains("acme ("), "{}", err.message);
        assert!(err.message.contains("dmm ("), "{}", err.message);
        let err = pick_tenant(&[], "nope").unwrap_err();
        assert!(err.message.contains("none"), "{}", err.message);
    }
}
