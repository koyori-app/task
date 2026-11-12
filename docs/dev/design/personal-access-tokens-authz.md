---
title: 個人アクセストークンの認可
description: PAT と Device Token の認証、操作スコープとリソース束縛の 2 層モデル、Desktop 認証
updated: 2026-11-04
sidebar:
  order: 2
---

## 概要

PAT による API 認証と、テナント・プロジェクトを横断しない権限チェックの設計・実装を記述する。認証方式は次の 3 つである。

- セッション（Cookie）: 全操作スコープ相当。テナント切り替えは UI から可能
- PAT（Bearer）: 作成時に固定した 1 テナントと任意のプロジェクト一覧の範囲内でのみ有効。操作は `scopes` で制限する
- Device Token（Bearer）: Koyori Desktop 用。権限はセッション相当で、セッション専用のエンドポイントだけは通さない（[Desktop 認証](#desktop-認証device-token)）

認可は操作スコープ（何ができるか）とリソース束縛（どこでできるか）の 2 層で行う。

関連するコードは `crates/entity/src/personal_tokens.rs`、`crates/entity/src/device_tokens.rs`、`crates/entity/src/scopes.rs`、`crates/handler/src/extractors.rs`、`crates/service/src/auth.rs`、`crates/service/src/desktop_auth.rs`、`crates/handler/src/handlers/desktop_auth.rs` である。

## 認証方式ごとの動作

動作は次のとおりである。

| 認証方式 | 操作スコープ | リソース範囲 |
|---|---|---|
| セッション | `require_scope` は常に通過 | 所属しているテナント（オーナー or テナントメンバー）。テナントに居なくても、明示 ACE のあるプロジェクトの中はゲストとして通る |
| PAT | DB の `scopes` を検証 | 作成時の `tenant_id` のみ。プロジェクトは任意指定 |
| Device Token | `require_scope` は常に通過 | セッションと同じ（束縛なし。所属判定は `has_tenant_access`） |

バインドは所属の証明ではない。PAT の `tenant_id` は「どのテナントを触れるか」の上限であって、その利用者がいまテナントに所属していることを意味しない。所属判定はセッションと同じ経路（`has_tenant_access`）を通るため、テナントから外した利用者のトークンはバインド先のテナントでもテナント全体のエンドポイントでは 403 になる。明示 ACE（`project_members` の行）が残るプロジェクトの中だけはゲストとして通る。詳細は[テナントとプロジェクトの認可](/dev/design/tenant-project-authz)に書く。

## PAT のリソース束縛

方針は次のとおりである。

| 項目 | 方針 |
|---|---|
| テナント | 作成時に 1 件必須（`tenant_id`）。他テナントの API は 403 |
| プロジェクト | 任意指定（`allowed_project_ids`）。未指定（`NULL`）は当該テナント内の全プロジェクト |
| 複数テナント PAT | 採用しない |
| テナント非紐づけ PAT | 採用しない（`/v1/auth/me` 等アカウント API は PAT では通らない。テナント横断が要る Desktop は別種別の Device Token を使う） |
| テナント作成 | セッション専用（PAT はテナントにバインドされているため新規作成不可） |
| PAT 管理 API（一覧・作成・失効） | セッション専用（PAT では不可）。一覧は自分の `revoked = false` のトークンだけを返す |
| PAT の作成とテナントの 2FA 強制 | 作成は `tenant_members` と同じく `ensure_tenant_access` を通す。確かめるテナントは、PAT を結び付けるテナント（要求の `tenant_id`）である。テナントが 2FA を強制していれば、2FA を満たさない利用者は 403（`2fa-setup-required`）になる。強制の前に取った本認証のセッションでも同じである |
| PAT の一覧・取り消しとテナントの 2FA 強制 | 強制を確かめない。一覧はテナントを跨いで自分のトークンを返す。取り消しは自分のトークンを減らす操作なので、強制の下でも消せる方が安全である。revoke-all はオーナー専用のまま |

## 発行の認可（誰が作れるか）

発行（`POST /v1/personal_tokens`）はテナントオーナーとテナント Admin に許す（`require_tenant_admin`。メンバー管理と同じ判定）。

Admin まで通す理由は次のとおりである。PAT は「作った本人として」動くトークンであり（`user_id` は発行者に固定）、各 API は要求のたびに `auth.user_id` に対して役割を再評価する。発行によって得られる権限は発行者が既に持つ権限を超えない。リソース束縛（`tenant_id` / `allowed_project_ids`）とスコープの検証は発行後の認可で掛かるため、発行のエンドポイントをオーナーに限っても守れるものが増えない。決定はこの理由だけで成り立つ。Admin の立場については、Admin は既にメンバー管理でテナントへの参加を与えられる立場にある（`require_tenant_admin` が守るメンバー管理のエンドポイントと同じ判定）という事実の記述に留める。

当初このエンドポイントはオーナー専用（ハンドラ内の私的な `require_tenant_owner`）だったが、その理由はどこにも書かれていなかった。理由の無い制限は、広げてよいのか守るべき境目なのかを後から判別できなくする。Admin が鍵を作れず 403 になった問題の元はこの無記録であり、同じことを繰り返さないためにこの節を置く。

revoke-all（`DELETE /v1/personal_tokens/revoke-all`）はテナント配下の他人のトークンも含めて失効させるエンドポイントであり、オーナー専用のまま変えていない。広げるべきかは未決である（[未実装と既知の制限](#未実装と既知の制限)）。

## 二層モデル

### Layer 1: 操作スコープ

「何ができるか」を `personal_tokens.scopes`（`ScopeList`）で表現する。GitLab の PAT と同じく、全部を任せる `api` / 読むだけの `read_api` と、資源ごとの `read:<資源>` / `write:<資源>` を並べる。

| スコープ | 意味 |
|---|---|
| `api` | すべての `read:*` / `write:*`（と `read_api`）を満たす |
| `read_api` | すべての `read:*` を満たす。`write:*` は満たさない |
| `read:tenant` | テナントとメンバーの参照（テナント一覧・取得、メンバー一覧、明示 ACE の一覧、利用者のプロフィール） |
| `write:tenant` | テナントの更新・削除、メンバーの追加・変更・削除、ドライブ容量の設定（`read:tenant` を含む） |
| `read:project` | プロジェクトの参照 |
| `write:project` | プロジェクトの作成・更新、プロジェクトメンバーと Webhook の管理（`read:project` を含む）。削除はセッション専用で PAT では通らない |
| `read:<資源>` / `write:<資源>` | task / milestone / sprint / review / drive の参照・変更 |
| `read:task`（補足） | スケジュール `GET /v1/tenants/{tenant_id}/schedule` もこれだけで読める。期間に重なるスプリント・マイルストーンも `read:sprint` / `read:milestone` なしで返す |

#### 含意の規則

包含関係は `Scope::implies`（持っている側の網羅 match）の一箇所に集める。`has_scope` は保持スコープのいずれかが要求スコープを含意するかを見るだけで、判定を散らさない。

| 持っているスコープ | 満たせる要求 |
|---|---|
| `api` | すべて |
| `read_api` | 動詞が read のスコープすべて（`Scope::verb`） |
| `write:<資源>` | 同じ資源の `read:<資源>`（tenant と project を含む 7 対すべて） |
| `read:<資源>` | 自分自身だけ |

各スコープの動詞（read / write / wildcard）は `Scope::verb` に網羅 match で置く。catch-all を置かないので、スコープを増やすと動詞と含意の規則を決めるまでコンパイルが通らない。`api` / `read_api` は動詞を単位に効くので、資源を増やしても既存の鍵が黙って権限を欠くことはない。

スコープ文字列にテナント ID を埋め込まない（例: `read:tenant:uuid` は採用しない）。`/me` 等アカウント API はセッション専用のため `read:user` / `write:user` は存在しない。

`api` とリソース束縛を組み合わせると、`allowed_project_ids` の指定で「指定プロジェクトの中だけで全操作ができる」トークンになる。束縛外とテナント全体のエンドポイントは 403 である。project-only のゲスト（#688）が `api` の PAT を使う場合も、通る範囲は所属判定（明示 ACE のあるプロジェクトの中だけ）で決まり、スコープが所属を広げることはない。

#### `api` / `read_api` への移行（TASK-199）

`admin:tenant` と `admin:project` を廃止し、`api` / `read_api` に置き換えた。`admin:project`（project 層の wildcard）は `write:project` と違いが分かりにくく、`admin:tenant` は全部を通す以外に意味を持たなかった。動詞で切る `api` / `read_api` なら名前から効き目が読める。

- テナント層のエンドポイントは `admin:tenant` でしか守られていなかったので、`read:tenant` / `write:tenant` を足して他の資源と同じ形にした。`api` はこれも満たす
- Webhook の管理（作成・更新・削除・配信履歴・再送）は `write:project` + プロジェクト Admin にした。プロジェクトの設定変更であり、`write:project` と分ける理由が無い
- 既存トークンはマイグレーション（[m20261004000000_pat_scopes_api.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261004000000_pat_scopes_api.rs)、SQL は `sql/migrate_pat_scopes_api.sql`）で移した。`admin:tenant` は `api` に移した（どちらも全部を満たす）。`admin:project` は project 層の `write:*` 6 個（project / drive / task / milestone / sprint / review）に移した。`api` に移すとテナント層まで広がるので、同じ範囲に展開した

### Layer 2: リソース束縛

「どこでできるか」を DB カラムで表現する。

| カラム | 型 | 説明 |
|---|---|---|
| `tenant_id` | `UUID` NOT NULL | PAT が有効なテナント（1 件固定） |
| `allowed_project_ids` | `JSON` NULL 可 | 許可プロジェクト ID の配列。`NULL` はテナント内全プロジェクト |

## PAT 自身の識別

PAT を使う CLI は `GET /v1/personal_tokens/me` で、使用中の鍵と持ち主を識別する。応答は鍵の ID と名前、持ち主の user ID と username、scopes、allowed_project_ids、有効期限に限る。アカウント情報の `email`、`has_password`、`totp_enabled`、`is_admin` は、鍵を識別するために要らないので返さない。

この endpoint は PAT 専用であり、Bearer がない要求と、認証済みの session Cookie の要求には 401 を返す。ただし `AuthUser` の拒否はそのまま外へ出るため、凍結された利用者の PAT には 403（`account-suspended`）、2FA が途中の session Cookie には 403（`forbidden`）が返る。`GET /v1/auth/me` と `PATCH /v1/auth/me` は PAT に開かない。PAT の識別はこの endpoint で行う。

`PATCH` は session 専用の `CurrentUser` のまま保つ。session extractor に認証方式の条件分岐を戻すと、Bearer を必ず拒む境界が再び曖昧になるためである。`GET` だけは Desktop のアカウント表示のため `AuthUser` + `require_session_or_device_token` で Device Token を通す（PAT は 403）。`CurrentUser` は変えず、`AuthUser` は Bearer があれば Cookie へフォールバックしないので、この境界は保たれる。

この endpoint は scope を要求しない。scope は鍵が実行できる操作を制限するための値であり、鍵自身の識別を拒むための値ではない。scope が空の鍵や狭い鍵ほど設定確認が必要になるため、認証に成功した PAT なら現在の制限をそのまま確認できる。

## 認証・認可の実装

### データ構造

`AuthUser` は `user_id` と `method`（`AuthMethod`）を持つ。`AuthMethod` の値は次のとおりである。

| 値 | 持つもの |
|---|---|
| `Session` | なし |
| `PersonalToken` | `token_id`、`tenant_id`、`allowed_project_ids`（`Option<Vec<Uuid>>`）、`scopes`（`ScopeList`） |
| `DeviceToken` | `token_id` |

### 認証フロー（`FromRequestParts`）

順序は次のとおりである。

1. `Authorization: Bearer kdt_…` なら Device Token 認証（[Desktop 認証](#desktop-認証device-token)）
2. それ以外の `Authorization: Bearer <token>` は PAT 認証
3. Bearer がなければ Cookie セッションから `user_id` 取得
4. どれもなければ 401

PAT 認証（`crates/service/src/auth.rs` の `authenticate_personal_token`）の順序は次のとおりである。

1. Bearer トークンを HMAC-SHA256 ハッシュ化する（`PERSONAL_TOKEN_SECRET` は `Settings` 経由で起動時に検証済み）
2. `personal_tokens` を `token_hash` で 1 SELECT する
3. 失効・期限切れをチェックする
4. 利用者の凍結など、認証での拒否をすべて通ったあと、前回の使用から 5 分以上空いていれば `last_used_at` を更新する。拒否された要求では更新しない。同時に来た要求が重ねて書くことがあり、書き込みの回数は厳密には保証しない
5. `AuthMethod::PersonalToken { ... }` を構築する。以降の認可はメモリのみで行う

### エンドポイントでの認可

ハンドラ先頭でヘルパーを呼ぶ。スコープ専用エクストラクタは作らない。呼び方は次のとおりである。

```rust
// テナント + プロジェクト両方を守る場合
auth.require_scope(Scope::ReadProject)?;
auth.ensure_tenant_access(&state, tenant_id, Some(project_id)).await?;

// テナントオーナー専用操作
auth.require_scope(Scope::WriteTenant)?;
let tenant = auth.ensure_tenant_owner(&state, tenant_id).await?;
```

### 認可ヘルパー

ヘルパーごとの動作は次のとおりである。

| メソッド | セッション | PAT | Device Token |
|---|---|---|---|
| `require_scope` | 常に OK | `scopes` を検証。不足なら 403 | 常に OK |
| `ensure_tenant_access` | 所属判定（`has_tenant_access`） | `token.tenant_id == path.tenant_id` + `allowed_project_ids` の突き合わせ（メモリ）の**あと**、セッションと同じ所属判定 | セッションと同じ |
| `ensure_tenant_owner` | `owner_id` チェック（1 SELECT） | `token.tenant_id` 一致 + プロジェクト制限なし + `owner_id` チェック | セッションと同じ |
| `require_session` | OK | 403 | 403 |
| `require_session_or_device_token` | OK | 403 | OK |

### HTTP ステータス

状況ごとのステータスは次のとおりである。

| 状況 | ステータス |
|---|---|
| 未認証・無効 PAT・失効・期限切れ | 401 |
| 認証済みだがスコープ / テナント / プロジェクト不一致 | 403 |
| 有効な Bearer（PAT・Device Token）でセッション専用のエンドポイントを呼んだ | 403 |
| テナントが 2FA を強制（`require_2fa`）し、人の利用者が 2FA を満たさない（TOTP もパスキーも無い） | 403（`2fa-setup-required`） |

BOT（`account_kind = bot`）は 2FA の強制の対象から外す（`ensure_tenant_2fa`）。BOT は TOTP もパスキーも持てず、セッションにも入れないので、強制の下では業務のエンドポイントを一つも使えなくなる。2FA は人の対話のサインインを守る物で、BOT は PAT でしか入らない。BOT の PAT を発行できるのは、テナントの 2FA の強制を満たした人の Admin だけである。BOT 管理のエンドポイントも `ensure_tenant_access` を通すので、強制の意図は発行の所で保つ。人の PAT の扱いは変えない。

Bearer への 401 は、トークンが無効（存在しない・失効・期限切れ・全失効）のときだけである。認証方式の不一致は 403 である。Desktop は Device Token への 401 を失効とみなしてログアウトするためである。

## Desktop 認証（Device Token）

Koyori Desktop はテナント横断の通知一覧（`/v1/users/me/notifications`）を読む必要があるが、PAT は 1 テナント束縛で、上の「テナント非紐づけ PAT は採用しない」を覆したくない。そこで PAT とは別種別の Device Token を足し、権限をセッション相当にする。

### 規則

規則は次のとおりである。

| 項目 | 規則 |
|---|---|
| 形式 | `kdt_<32 バイト乱数の base64url>`。接頭辞で PAT（`pat_`）と判別する |
| 保存 | `device_tokens`。平文は持たず、PAT と同じ HMAC-SHA256（`PERSONAL_TOKEN_SECRET`）の hash を `token_hash`（UNIQUE）で引く。列の定義は [m20260924010000_device_tokens.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260924010000_device_tokens.rs) にある |
| 有効期限 | 発行から 90 日（`expires_at`）。期限切れは 401 で、Desktop はブラウザ承認をやり直す |
| 失効 | `revoked_at` を立てる（行は残す）。失効後は 401 |
| 全端末の失効 | `users.sessions_revoked_at` より前に発行（`created_at`）したものは 401。パスワード変更で全端末が落ちる |
| 凍結 | 凍結された利用者は 403（`account-suspended`）。PAT と同じ |
| 権限 | スコープ・テナント束縛はセッションと同等。`require_session` のエンドポイント（PAT 管理・テナント作成・更新・削除、プロジェクト削除、GitHub 連携の管理・2FA・認可コードの発行）は 403 |
| 通知 | `/v1/users/me/notifications` 系は `require_scope`（Device Token は常に通過）+ 視界の絞り込み（`visible_project_ids_for`）。Device Token の視界はセッションと同じで、本人が入れるプロジェクトすべてと `project_id` を持たない古い通知を含む。PAT のテナント絞り込みは掛けない（[通知](#v1usersme通知)） |
| 端末管理 | `/v1/users/me/devices` 系はセッションと Device Token だけ（`require_session_or_device_token`）。PAT は 403 |
| `/v1/auth/me` | `GET` はセッションと Device Token（`require_session_or_device_token`）。PAT は 403。Desktop のアカウント表示に使う。`PATCH` はセッション専用（有効な Bearer は 403） |
| `last_used_at` | 認証成功時に更新する。Desktop は 30 秒ごとにポーリングするので、前回から 5 分以上空いたときだけ書く |

### フロー（Authorization Code + PKCE、loopback）

流れは次のとおりである。

```text
Desktop                                   Browser / Web              Backend
  │ 127.0.0.1:{port} で待ち受け
  │ code_verifier / code_challenge / state を生成
  ├─ 開く ─→ {web}/desktop/authorize?port=&code_challenge=&state=&name=
  │                                          │ 未ログインならログイン後に戻る
  │                                          │ 「Koyori Desktop を承認」画面
  │                                          ├─ POST /v1/desktop/auth/codes ──→ セッション必須
  │                                          │←──── {code}
  │←─ redirect http://127.0.0.1:{port}/callback?code=&state= ─┤
  │ state を検証
  ├─ POST /v1/desktop/auth/token {code, code_verifier} ────────────────────→
  │←─────────────────────── {token, expires_at, device_id} ────────────────┤
```

| 項目 | 規則 |
|---|---|
| code | 32 バイト乱数。Redis（`desktop_auth:code:{code}`）に TTL 5 分で `{user_id, code_challenge, name, issued_at_ms}` を置く。GETDEL で一度きり |
| PKCE | S256 のみ。`code_challenge` は base64url の 43〜128 文字。`BASE64URL(SHA256(code_verifier))` が一致しなければ 401 で、code は GETDEL 済みなので二度と使えない |
| 発行のエンドポイント | セッション専用。2FA 途中のセッション・PAT・Device Token は 403 |
| 交換のエンドポイント | 未認証（code が資格）。不一致・期限切れ・再利用・発行時刻を持たない旧コード・停止中の利用者はすべて 401。`sessions_revoked_at` がコード発行時刻以降なら未交換でも拒否する |
| レート制限 | 交換のエンドポイントに接続元（`X-Forwarded-For`、無ければ `X-Real-IP`）ごと 10 回/分。超えたら 429。ヘッダは偽装できるので多重防御の 1 つである（code は 256 bit で総当たりは成立しない） |
| redirect | loopback（`http://127.0.0.1:{port}/callback`）のみ。Web の承認画面は `port` を 1024〜65535 の整数に限る。Custom URI Scheme は使わない（Linux で登録が要り、Windows でレジストリ書き込みが要り、他アプリに奪われ得る） |
| ログ | code / code_verifier / token を出さない（リクエストログはパスのみ） |

長寿命の資格情報を URL に載せない。URL に出るのは短寿命の code だけである。Device Token の `created_at` はコードの承認時刻を引き継ぐ。交換処理中に全失効が走った場合も、Bearer 認証でその失効を検出する。有効期限は交換時点から 90 日である。

### エンドポイント

一覧は次のとおりである。

| メソッド | パス | 認証 | 内容 |
|---|---|---|---|
| `POST` | `/v1/desktop/auth/codes` | セッション | `{code_challenge, name}`（name は 1〜100 文字）を受け、201 `{code}` を返す |
| `POST` | `/v1/desktop/auth/token` | なし | `{code, code_verifier}` を受け、201 `{token, expires_at, device_id}` を返す。平文はこの応答でのみ返す |
| `GET` | `/v1/users/me/devices` | セッション / Device Token | 自分の有効な端末（失効済み・期限切れを除く）を新しい順に返す |
| `DELETE` | `/v1/users/me/devices/{id}` | セッション / Device Token | `revoked_at` を立てる。Device Token は自身のみ（= ログアウト）、セッションは自分の他の端末も失効可。他人の端末は 404 |

CSRF については、Bearer 付きの要求は Origin 検査の対象外である（Device Token も同じ）。発行のエンドポイントはセッション Cookie で呼ぶので、Origin 検査を通る。

## API パス規約

テナント配下のリソースは path にテナント（・プロジェクト）を含める。形は次のとおりである。

```text
GET /v1/tenants/{tenant_id}/projects/{project_id}/...
```

path の ID と PAT の `tenant_id` / `allowed_project_ids` を突き合わせる。

path にテナントを持たない Drive の本体取得（`GET /v1/drive/files/{id}/content`）は、ファイル自身の `tenant_id` / `project_id` で同じ束縛を突き合わせる（`ensure_token_binding`。共有トークン `?token=` の経路は対象外）。

アカウント API（例: `GET /v1/auth/me`）は PAT 非対応である。セッションのみで、`GET /v1/auth/me` は Device Token も通す。

### `/v1/users/me/...`（通知）

通知の 5 エンドポイントは path にテナントを持たないが、PAT で使える（CLI の `task notifications`）。path から束縛を突き合わせられないぶん、結果の側をトークンの束縛まで絞る。

| メソッド | パス | 必要スコープ | PAT のときの範囲 |
|---|---|---|---|
| `GET` | `/v1/users/me/notifications` | 種別に応じて `read:task` / `read:review` | PAT のテナントのプロジェクト ∩ `allowed_project_ids` ∩ 読める種別（一覧・`unread_count` の双方） |
| `PATCH` | `/v1/users/me/notifications/{id}/read` | 種別に応じて `write:task` / `write:review` | 同じプロジェクト範囲で書き込める種別。範囲外は 404 |
| `PATCH` | `/v1/users/me/notifications/read-all` | 種別に応じて `write:task` / `write:review` | 同上の範囲だけ既読にする |
| `GET` | `/v1/users/me/notification-settings/{project_id}` | `read:task` | `ensure_tenant_access`（プロジェクトの束縛を突き合わせる） |
| `PUT` | `/v1/users/me/notification-settings/{project_id}` | `write:task` | 同上 |

絞り込みは所属判定（セッションと同じ経路）との積で、バインドが所属を広げることはない。`review_round_created` / `review_finding_changed` はレビューのスコープを必要とする。その他の既知の通知はタスクのスコープを使い、未知の種別は PAT に公開しない。各操作でタスク・レビューのどちらの必要スコープもなければ 403 である。書き込みスコープによる読み取り権限の包含と、`api` / `read_api` の包含も適用する。

`project_id` を持たない古い通知は、どのプロジェクトのものか判別できないのでセッションと Device Token にだけ見せる。Device Token はテナントに束縛しないので、視界はセッションと同じである。

### `/v1/tenants/{tenant_id}/projects/{project_id}/webhooks`（外部向け Webhook）

読み取りは `read:project`、変更は `write:project` である。変更はスコープに加えて、利用者本人がそのプロジェクトの Admin・テナントオーナー・そのプロジェクトに入れるテナント `Admin` のいずれかであること（`require_project_admin`）が要る。PAT に `write:project` を付けても、発行者が Member なら 403 である。

| メソッド | パス | 必要スコープ |
|---|---|---|
| `GET` | `/webhooks` | `read:project` |
| `POST` | `/webhooks` | `write:project` + プロジェクト Admin |
| `PUT` | `/webhooks/{id}` | `write:project` + プロジェクト Admin |
| `DELETE` | `/webhooks/{id}` | `write:project` + プロジェクト Admin |
| `GET` | `/webhooks/{id}/deliveries` | `write:project` + プロジェクト Admin |
| `POST` | `/webhooks/{id}/deliveries/{did}/redeliver` | `write:project` + プロジェクト Admin |

## DB アクセス回数

目安は次のとおりである。

| 認証 | 認可まわりの DB（目安） |
|---|---|
| PAT | 認証時 `personal_tokens` 1 SELECT（前回の使用から 5 分以上空いたときだけ、あわせて 1 UPDATE）。`require_scope` とバインドの突き合わせは追加クエリなし。所属判定はセッションと同じクエリを流す |
| セッション | Redis セッション + 所属判定（テナント 1 SELECT + `tenant_members` 1 SELECT。プロジェクト指定時は最大 3 SELECT 追加） |

避けることは次のとおりである。

- `require_*` ごとに DB を再クエリする
- ハンドラごとに Bearer パース + トークン lookup を重複する

## 採用しない方針

採用しないものは次のとおりである。

- セッションにスコープを持たせる
- 複数テナント / テナント非紐づけ PAT
- スコープ専用エクストラクタ
- Route Layer での一括スコープ管理
- スコープ文字列へのリソース ID 埋め込み
- `scopes` だけでテナント境界を守る（必ず `tenant_id` カラムと併用）

## 参考

- GitHub: PAT scopes + fine-grained PAT の repository 指定
- GitLab: project access token

## 未実装と既知の制限

- revoke-all をオーナー以外に広げるべきかは未決である

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-05-17 | [採用しない方針](#採用しない方針)の 6 項目を採らない | 記録なし |
| 2026-09-13 | PAT の発行をテナントオーナーからテナント Admin まで広げる | 発行で得られる権限は発行者が既に持つ権限を超えず、束縛とスコープの検証は発行後の認可で掛かる（[発行の認可](#発行の認可誰が作れるか)） |
| 2026-09-13 | PAT の識別は `GET /v1/personal_tokens/me` で行い、scope を要求しない | scope は鍵が実行できる操作を制限する値であり、鍵自身の識別を拒む値ではない（[PAT 自身の識別](#pat-自身の識別)） |
| 2026-09-24 | Desktop には PAT と別種別の Device Token を足し、権限をセッション相当にする | テナント横断の通知一覧が要るが、「テナント非紐づけ PAT は採用しない」を覆したくない |
| 2026-09-24 | Desktop の redirect に Custom URI Scheme を使わず loopback に限る | Linux で登録が要り、Windows でレジストリ書き込みが要り、他アプリに奪われ得る |
| 2026-10-04 | `admin:tenant` と `admin:project` を廃止し、`api` / `read_api` に置き換える（TASK-199） | 動詞で切る `api` / `read_api` なら名前から効き目が読める |
| 2026-11-11 | PAT の `last_used_at` を認証成功時に同期で更新し、前回の使用から 5 分以内の再使用では書かない（TASK-492） | 一覧の最終使用が常に未使用だった。CLI や BOT の連続した要求で毎回は書かないため。認証後の認可 403 は使用に含め、Device Token にそろえる |
