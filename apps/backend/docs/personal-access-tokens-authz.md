# パーソナルアクセストークン（PAT）認可

**関連コード**: `src/entities/personal_tokens.rs`, `src/entities/scopes.rs`, `src/extractors.rs`, `src/utils/auth.rs`

## 概要

PAT による API 認証と、テナント・プロジェクトを横断しない権限チェックの設計・実装を記述する。

- **セッション（Cookie）**: 全操作スコープ相当。テナント切り替えは UI から可能。
- **PAT（Bearer）**: 作成時に固定した **1 テナント** と **任意のプロジェクト一覧** の範囲内でのみ有効。操作は `scopes` で制限。

認可は **操作スコープ（何ができるか）** と **リソース束縛（どこでできるか）** の 2 層で行う。

## 認証方式ごとの動作

| 認証方式 | 操作スコープ | リソース範囲 |
|----------|-------------|--------------|
| セッション | `require_scope` は常に通過 | 所属しているテナント（オーナー or テナントメンバー）。テナントに居なくても、明示 ACE のあるプロジェクトの中は客分として通る |
| PAT | DB の `scopes` を検証 | 作成時の `tenant_id` のみ。プロジェクトは任意指定 |

**バインドは所属の証明ではない。** PAT の `tenant_id` は「どのテナントを触れるか」の上限であって、
その利用者がいまテナントに所属していることを意味しない。
所属判定はセッションと同じ経路（`has_tenant_access`）を通るため、テナントから外した利用者のトークンは
バインド先のテナントでもテナント全体の口では 403 になる。明示 ACE（`project_members` の行）が残るプロジェクトの中だけは
客分として通る。詳細は [tenant-project-authz.md](./tenant-project-authz.md) を参照。

## PAT のリソース束縛

| 項目 | 方針 |
|------|------|
| テナント | 作成時に **1 件必須**（`tenant_id`）。他テナントの API は 403 |
| プロジェクト | **任意指定**（`allowed_project_ids`）。未指定（`NULL`）= 当該テナント内の全プロジェクト |
| 複数テナント PAT | 採用しない |
| テナント非紐づけ PAT | 採用しない（`/me` 等アカウント API はセッション専用） |
| テナント作成 | **セッション専用**（PAT はテナントにバインドされているため新規作成不可） |
| PAT 管理 API（一覧・作成・失効） | **セッション専用**（PAT では不可）。一覧は自分の `revoked = false` のトークンだけを返す |

## 発行の認可（誰が作れるか）

発行（`POST /v1/personal_tokens`）は **テナントオーナーとテナント Admin** に許す
（`require_tenant_admin`。メンバー管理と同じ判定手）。

**なぜ Admin まで通すのか**: PAT は「作った本人として」動くトークンであり
（`user_id` は発行者に固定）、各 API は要求のたびに `auth.user_id` に対して役を
再評価する。発行によって得られる力は発行者が既に持つ力を超えない。
リソース束縛（`tenant_id` / `allowed_project_ids`）とスコープの検証は発行後の認可で
掛かるため、発行の口を主に限っても守れる物が増えない——決定はこの一本で支えられる。
Admin の立場に触れておくなら、Admin は既にメンバー管理でテナントへの参加を
与えられる立場にある（`require_tenant_admin` が守るメンバー管理の口と同じ判定手）、
という事実の記述に留める。

当初この口はオーナー専用（ハンドラ内の私物 `require_tenant_owner`）だったが、
**その理由はどこにも書かれていなかった**。理由の無い制限は、広げてよいのか
守るべき境目なのかを後から判別できなくする——今回の躓き（Admin が鍵を作れず 403）の
元はこの無記録であった。この節はその轍を避けるために在る。

なお revoke-all（`DELETE /v1/personal_tokens/revoke-all`）はテナント配下の
**他人のトークンも含めて** 失効させる口であり、オーナー専用のまま変えていない。
広げるべきかは別途の裁きに委ねる。

## 二層モデル

### Layer 1: 操作スコープ

「何ができるか」を `personal_tokens.scopes`（`ScopeList`）で表現する。

| スコープ | 意味 |
|---------|------|
| `read:project` | プロジェクトの参照 |
| `write:project` | プロジェクトの更新（`read:project` を含む） |
| `admin:project` | project 層の全スコープ（下表参照）を包含する wildcard。tenant 層は満たさない |
| `admin:tenant` | 当該 PAT の `tenant_id` で行える操作すべて（層に依らない最上位の wildcard） |

`admin:tenant` を持つトークンはすべての `require_scope` チェックを通過する（`ScopeList::has_scope` 参照）。

#### 含意の規則

包含関係は `Scope::implies`（持っている側の網羅 match）の一箇所に集める。`has_scope` は
保持スコープのいずれかが要求スコープを含意するかを見るだけで、判定を散らさない。

| 持っているスコープ | 満たせる要求 |
|---|---|
| `admin:tenant` | すべて（層に依らない） |
| `admin:project` | project 層のスコープすべて |
| `write:<資源>` | 同じ資源の `read:<資源>`（project を含む 6 対すべて） |
| `read:<資源>` | 自分自身だけ |

catch-all を置かないので、スコープを増やすと含意の規則を決めるまでコンパイルが通らない。

#### スコープの層

`admin:project` の効き目を限るために、各スコープを層へ割り振る（`Scope::layer`）。
スコープを増やしたら必ずどちらかへ割り振る。

| 層 | スコープ |
|----|---------|
| project 層 | `read:project` / `write:project` / `read:task` / `write:task` / `read:milestone` / `write:milestone` / `read:sprint` / `write:sprint` / `read:review` / `write:review` / `read:drive` / `write:drive` / `admin:project` |
| tenant 層 | `admin:tenant` |

この表は各スコープの帰属であって、`admin:tenant` の効き目の範囲ではない。層は
`admin:project` を限る道具であり、`admin:tenant` を限る道具ではない。層を増やしても
`admin:tenant` は通る。

**決めたこと**: `admin:tenant` ⊃ `admin:project` とする。現行の `admin:tenant` wildcard は
「要求されたスコープが何であれ通す」意味論であり、`admin:project` の要求もこれに含まれるため、
包含しない形にすると wildcard の意味論を曲げることになる。逆向き（`admin:project` が
`admin:tenant` を満たす）は無い。

**決めたこと**: 層と `admin:tenant` の関係は、実装ではなく記述の側を直して揃えた。
`admin:tenant` を「tenant 層かつ全層を包含」と定義し直す道もあったが、それは
「層に依らない」と同義であって層が `admin:tenant` を限る道具にならない点は変わらず、
既に 5 箇所（この文書の上下、drive.md、review-findings.md、PAT 作成 UI の文言）が
「層に依らない最上位」で揃っている。実装を動かす利は無い。

**決めたこと**: `write:project` は `read:project` を含意する。他の 5 対（task / drive /
milestone / sprint / review）は当初からそう扱っており、project だけが対を欠いていた。
`write:project` を発行できる者は `admin:project` も発行でき、そちらは `read:project` を
満たすため、非対称は防御になっておらず不整合でしかなかった。

`admin:project` とリソース束縛の組み合わせ: `allowed_project_ids` を指定すれば
「指定プロジェクトの中だけで project 層の全操作ができる」トークンになる（束縛外は従来どおり 403）。
project-only の客分（#688）が `admin:project` の PAT を使う場合も、通る範囲は所属判定
（明示 ACE のあるプロジェクトの中だけ）で決まり、スコープが所属を広げることはない。

スコープ文字列にテナント ID を埋め込まない（例: `read:tenant:uuid` は採用しない）。  
`/me` 等アカウント API はセッション専用のため `read:user` / `write:user` は存在しない。

### Layer 2: リソース束縛

「どこでできるか」を DB カラムで表現する。

| カラム | 型 | 説明 |
|--------|-----|------|
| `tenant_id` | `UUID` NOT NULL | PAT が有効なテナント（1 件固定） |
| `allowed_project_ids` | `JSON` NULL 可 | 許可プロジェクト ID の配列。`NULL` = テナント内全プロジェクト |

## PAT 自身の識別

PAT を使う CLI は `GET /v1/personal_tokens/me` で、使用中の鍵と持ち主を識別する。
応答は鍵の ID と名前、持ち主の user ID と username、scopes、allowed_project_ids、有効期限に限る。
アカウント情報の `email`、`has_password`、`totp_enabled`、`is_admin` は、鍵を識別するために要らないので返さない。

この endpoint は PAT 専用であり、Bearer がない要求と、認証済みの session Cookie の要求には 401 を返す。
ただし `AuthUser` の拒否はそのまま外へ出るため、凍結された利用者の PAT には 403（`account-suspended`）、2FA が途中の session Cookie には 403（`forbidden`）が返る。
`GET /v1/auth/me` と `PATCH /v1/auth/me` は session 専用のまま保つ。
既存の session endpoint に認証方式の条件分岐を戻すと、session extractor が Bearer を必ず拒む境界が再び曖昧になるため、二つの認証方式を同じ `me` endpoint へ通さない。

この endpoint は scope を要求しない。
scope は鍵が実行できる操作を制限するための値であり、鍵自身の識別を拒むための値ではない。
scope が空の鍵や狭い鍵ほど設定確認が必要になるため、認証に成功した PAT なら現在の制限をそのまま確認できる。

## 認証・認可の実装

### データ構造

```rust
pub enum AuthMethod {
    Session,
    PersonalToken {
        token_id: Uuid,
        tenant_id: Uuid,
        allowed_project_ids: Option<Vec<Uuid>>,
        scopes: ScopeList,
    },
}

pub struct AuthUser {
    pub user_id: Uuid,
    pub method: AuthMethod,
}
```

### 認証フロー（`FromRequestParts`）

1. `Authorization: Bearer <token>` があれば PAT 認証
2. なければ Cookie セッションから `user_id` 取得
3. どちらもなければ **401**

PAT 認証（`authenticate_personal_token` in `utils/auth.rs`）:

1. Bearer トークンを HMAC-SHA256 ハッシュ化（`PERSONAL_TOKEN_SECRET` は `Settings` 経由で起動時に検証済み）
2. `personal_tokens` を `token_hash` で 1 SELECT
3. 失効・期限切れチェック
4. `AuthMethod::PersonalToken { ... }` を構築（**以降の認可はメモリのみ**）

> **未実装**: `last_used_at` の fire-and-forget 更新

### エンドポイントでの認可

ハンドラ先頭でヘルパーを呼ぶ。スコープ専用エクストラクタは作らない。

```rust
// テナント + プロジェクト両方を守る場合
auth.require_scope(Scope::ReadProject)?;
auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;

// テナントオーナー専用操作
auth.require_scope(Scope::AdminTenant)?;
let tenant = auth.ensure_tenant_owner(&state, tenant_id).await?;
```

### 認可ヘルパー

| メソッド | セッション | PAT |
|----------|-----------|-----|
| `require_scope` | 常に OK | `scopes` を検証。不足なら 403 |
| `ensure_tenant_access` | 所属判定（`has_tenant_access`） | `token.tenant_id == path.tenant_id` + `allowed_project_ids` の突き合わせ（メモリ）の**あと**、セッションと同じ所属判定 |
| `ensure_tenant_owner` | `owner_id` チェック（1 SELECT） | `token.tenant_id` 一致 + プロジェクト制限なし + `owner_id` チェック |

### HTTP ステータス

| 状況 | ステータス |
|------|-----------|
| 未認証・無効 PAT・失効・期限切れ | 401 |
| 認証済みだがスコープ / テナント / プロジェクト不一致 | 403 |

## API パス規約

テナント配下のリソースは path にテナント（・プロジェクト）を含める。

```
GET /v1/tenants/{tenant_id}/projects/{project_id}/...
```

path の ID と PAT の `tenant_id` / `allowed_project_ids` を突き合わせる。  
アカウント API（例: `GET /v1/auth/me`）は PAT 非対応（セッションのみ）。

### `/v1/users/me/...`（通知）

通知の 5 口は path にテナントを持たないが、PAT で使える（CLI の `task notifications`）。
path から束縛を突き合わせられないぶん、**結果の側をトークンの束縛まで絞る**。

| メソッド | パス | 必要スコープ | PAT のときの範囲 |
|---------|------|-------------|-----------------|
| `GET` | `/v1/users/me/notifications` | 種別に応じて `read:task` / `read:review` | PAT のテナントのプロジェクト ∩ `allowed_project_ids` ∩ 読める種別（一覧・`unread_count` の双方） |
| `PATCH` | `/v1/users/me/notifications/{id}/read` | 種別に応じて `write:task` / `write:review` | 同じプロジェクト範囲で書き込める種別。範囲外は 404 |
| `PATCH` | `/v1/users/me/notifications/read-all` | 種別に応じて `write:task` / `write:review` | 同上の範囲だけ既読にする |
| `GET` | `/v1/users/me/notification-settings/{project_id}` | `read:task` | `ensure_tenant_access`（プロジェクトの束縛を突き合わせる） |
| `PUT` | `/v1/users/me/notification-settings/{project_id}` | `write:task` | 同上 |

絞り込みは所属判定（セッションと同じ経路）との積で、バインドが所属を広げることはない。
`review_round_created` / `review_finding_changed` はレビューのスコープを必要とする。
その他の既知の通知はタスクのスコープを使い、未知の種別は PAT に公開しない。
各操作でタスク・レビューのどちらの必要スコープもなければ 403。
書き込みスコープによる読み取り権限の包含と、管理スコープの包含も適用する。
`project_id` を持たない古い通知は、どのプロジェクトのものか判別できないのでセッション専用。

### `/v1/tenants/{tenant_id}/projects/{project_id}/webhooks`（外部向け Webhook）

読み取りは `read:project`、変更は `admin:project`。変更はスコープに加えて、利用者本人が
そのプロジェクトの Admin かテナントオーナーであること（`require_project_admin`）が要る。
PAT に `admin:project` を付けても、発行者が Member なら 403。

| メソッド | パス | 必要スコープ |
|---------|------|-------------|
| `GET` | `/webhooks` | `read:project` |
| `POST` | `/webhooks` | `admin:project` + プロジェクト Admin |
| `PUT` | `/webhooks/{id}` | `admin:project` + プロジェクト Admin |
| `DELETE` | `/webhooks/{id}` | `admin:project` + プロジェクト Admin |
| `GET` | `/webhooks/{id}/deliveries` | `read:project` |
| `POST` | `/webhooks/{id}/deliveries/{did}/redeliver` | `admin:project` + プロジェクト Admin |

## DB アクセス回数

| 認証 | 認可まわりの DB（目安） |
|------|------------------------|
| PAT | 認証時 `personal_tokens` **1 SELECT**。`require_scope` とバインドの突き合わせは追加クエリなし。所属判定はセッションと同じクエリを流す |
| セッション | Redis セッション + 所属判定（テナント 1 SELECT + `tenant_members` 1 SELECT。プロジェクト指定時は最大 3 SELECT 追加） |

避けること:

- `require_*` ごとに DB を再クエリする
- ハンドラごとに Bearer パース + トークン lookup を重複する

## テスト要件

- `ScopeList::has_scope`: `admin:tenant` は全スコープを通過、不足スコープは 403
- `ScopeList::has_scope`: `admin:project` は project 層の全スコープを通過し、tenant 層（`admin:tenant`）は通過しない（両向きを固定）
- `ScopeList::has_scope`: `write:<資源>` は同じ資源の `read:<資源>` を通過し、逆向きは通過しない。対はスコープ名から導いて全件を回す（写しを置かない）
- `require_scope`: Session は常に OK、PAT は不足で 403
- PAT が別テナントの path を叩く → 403
- `allowed_project_ids` 外の project → 403、`NULL` ならテナント内任意 project → OK
- 失効 / 期限切れ / ハッシュ不一致 → 401
- PAT 作成: 他人の `tenant_id` → 403

## 採用しない方針

- セッションにスコープを持たせる
- 複数テナント / テナント非紐づけ PAT
- スコープ専用エクストラクタ
- Route Layer での一括スコープ管理
- スコープ文字列へのリソース ID 埋め込み
- `scopes` だけでテナント境界を守る（必ず `tenant_id` カラムと併用）

## 参考

- GitHub: PAT scopes + fine-grained PAT の repository 指定
- GitLab: project access token
