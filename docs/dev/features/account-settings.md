---
title: アカウント設定
description: ログイン中の利用者が自分のプロフィール・アクセストークン・認証方法を管理する画面と API、他の利用者のプロフィール
status: implemented
updated: 2026-11-04
sidebar:
  order: 3
---

## 概要

ログイン中の利用者は、自分のプロフィール、パーソナルアクセストークン、認証方法、承認した Koyori Desktop の端末を管理できる。同じテナントに関わる人のプロフィールも見られる。編集対象は `users` の列のうち本人が持ち主である 3 つに限り、メールアドレス・管理者フラグ・凍結フラグ・2FA の有効状態はこの API では変更できない。`users` の内容を変更する API には、ほかに管理者専用の `PATCH /v1/admin/users/{id}` がある。アバターの保存は[ドライブのストレージバックエンド](/dev/features/drive#ストレージバックエンド)に依存する。

## 振る舞い

### 編集できる項目

編集できる項目は次のとおりである。

| 項目 | 列 | 備考 |
|---|---|---|
| ユーザー名 | `users.username` | 一意制約は無く、同名を許す |
| 自己紹介 | `users.bio` | |
| アバター | `users.avatar_url` | 画像をアップロードする（`POST /v1/auth/me/avatar`）か、画像の URL を指定する。アップロードした画像のキーは `users.avatar_storage_key` |

### 同名の利用者

`users.username` に一意制約は無く、`PATCH /v1/auth/me` も重複を確認しない。既存の利用者と同じ `username` に変更でき、登録 API（`POST /v1/auth/register`）も同様に重複を許す。本人によるプロフィール更新は監査ログに記録し、ユーザー名の変更前後を追跡できる。

メンションの宛先は username の一致では決めない。コメント・タスクの説明文のメンションはメンションのマーカー（`<@利用者ID>`）で利用者の ID を指す（[コメント・アクティビティ](/dev/features/tasks/collaboration)の @メンション処理）。同名の利用者が同じプロジェクトにいても補完で選んだ一人にだけ届き、改名してもマーカーは切れない。画面上では、コメント・担当者・`AvatarGroup` の表示名が同じになる。

### 省略と消去

送らなかったフィールドは変更しない。フィールドが 1 つも無い `{}` を送った場合も `200 OK` を返し、値は変わらない。指定の有無ではなく現在値との差を見るので、同じ値を送っても `UPDATE` と監査ログは発行しない。

`avatar_url` を消すときは `{ "clear_avatar_url": true }` を送る。空文字（`""`）では消せず、入力チェックで弾かれる。`bio` の消去は `{"bio": ""}` で行う。`bio` は空文字がそのまま有効な値で、新規登録時も空文字が入り、`NULL` と `""` をアプリ側で区別していない。

### アバターの URL

`avatar_url` はフロントエンドで `<img src>` に流し込むので、`https://` 以外を保存段階で弾く（`payload::users::validate_avatar_url`）。`javascript:`、`data:`、HTTP、相対パスは受け付けない。同じ判定をフロントエンドの `ProfileForm.vue` にも置いており、変更するときは両方を直す。前後の空白は、画面側の判定・送信値のどちらでも取り除く。

`avatar_url` は本人以外の画面でも `<img src>` に載る（担当者一覧の `AvatarGroup` など）。指定された外部 URL は閲覧者のブラウザが直接取りに行くので、その URL を用意した側はアバターを見た人の IP アドレスと User-Agent を知り得る。アップロードしたアバターは自サーバーから配信するので、この問題は無い。

アップロードで入る `avatar_url`（`{APP_BASE_URL}/v1/avatars/...`）は、`APP_BASE_URL` が http の開発環境では `https://` で始まらない。サーバーが書き込む値なのでこの制限は掛からず、画面側も現在値は検証しない。

### アバターのアップロード

`POST /v1/auth/me/avatar` は画像をアップロードし、`avatar_url` を自サーバーの配信 URL に差し替える。規則は次のとおりである。

- リクエストは `multipart/form-data` で、フィールド `file` を 1 つ送る
- 画像は 2 MiB（`service::avatar::AVATAR_MAX_BYTES`）までで、超えたら `413` を返す。本文全体の上限（`RequestBodyLimitLayer`）はこのルートだけに掛け、multipart の境界とヘッダーの分だけ 64 KiB 広げる。本文はメモリに読む
- 形式はクライアントの content-type を信用せず、先頭バイトで判定する（`service::avatar::detect_image_format`）。それ以外・空は `400`（`unsupported-avatar-format`）、`file` が無ければ `400` を返す
- ドライブと同じストレージバックエンドに `avatar-{uuid v4}.{拡張子}` のキーで置く。`drive_files` の行は作らず、リサイズ・寸法の検査・EXIF の除去はしない
- トランザクションで `avatar_url = {APP_BASE_URL}/v1/avatars/{key}`、`avatar_storage_key = key` にし、監査ログ `user.profile.update`（`changed_fields: ["avatar_url"]`、`PATCH /v1/auth/me` と同じ形）を残す
- DB の更新に失敗したら、置いた画像を消してからエラーを返す
- 応答は `200 OK` / `UserResponse` である

`avatar_url` の生成に使う `APP_BASE_URL` は OAuth のコールバック URL と同じ設定値で、ブラウザから届くバックエンドの公開 URL にする。

判定する形式は次のとおりである。

| 形式 | 先頭バイト | 拡張子 |
|---|---|---|
| PNG | `89 50 4E 47` | `png` |
| JPEG | `FF D8 FF` | `jpg` |
| GIF | `GIF8` | `gif` |
| WebP | `RIFF????WEBP` | `webp` |

### アバターの配信

`GET /v1/avatars/{key}` はアップロードした画像を認証なしで配信する。`<img src>` から取れる必要があり、Koyori Desktop も同じ URL を使う。キーは UUID v4 で推測できない。

- `key` が `^avatar-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(png|jpg|gif|webp)$` に一致しなければ `404` を返す
- DB は引かず、`Content-Type` は拡張子から決める。画像が無ければ `404` を返す
- ヘッダは `Content-Type`、`Cache-Control: public, max-age=31536000, immutable`、`Content-Disposition: inline` である。`X-Content-Type-Options: nosniff` は全経路共通の層で付く
- 差し替えるとキーごと変わるので、同じ URL の中身は変わらない

### 画像の片付け

使われなくなった画像は、DB の更新をコミットした後に消す。消すのに失敗しても応答は成功のままにし、`tracing::warn!` で記録する。孤児の画像が残るだけで、利用者から見える状態は正しい。

| 契機 | 消す画像 |
|---|---|
| 再アップロード | 直前の `avatar_storage_key` |
| `PATCH /v1/auth/me` で `clear_avatar_url: true`、または `avatar_url` を別の URL に変更 | 直前の `avatar_storage_key`。列は `NULL` に戻す |
| 管理者による利用者の削除（`DELETE /v1/admin/users/{id}`） | 対象の利用者の `avatar_storage_key` |

アバター以外の項目だけを変える `PATCH` では消さない。

### アクセストークン

PAT 自体の認可設計の定義元は[個人アクセストークンの認可](/dev/design/personal-access-tokens-authz)である。`GET /v1/personal_tokens` は自分（`user_id`）のトークンのうち `revoked = false` のものを、名前の昇順（同名は `id` 順）で `PersonalTokenResponse` の配列として返す。平文トークン・ハッシュは含まず、1 件も持たない場合は空配列を返す。

平文トークンは発行直後の応答でしか取得できない。`DELETE /v1/personal_tokens/{id}` は行を消さず `revoked` フラグを立て、冪等である。取り消したトークンは一覧に出なくなる。

### Koyori Desktop の端末

Device Token の規則の定義元は [Bearer 認証・認可の「Desktop 認証」](/dev/design/personal-access-tokens-authz)である。

- `GET /v1/users/me/devices` は、失効済み・期限切れと、パスワードの変更などで無効になった（`sessions_revoked_at` 以前に発行した）端末を除いて新しい順に返す
- `DELETE /v1/users/me/devices/{id}` は `revoked_at` を立てるだけで行は残り、一覧からは消える。その端末は次の要求から `401` になる

### 他の利用者のプロフィール

テナントのメンバーは、同じテナントに関わる人のプロフィールを見られる（TASK-257）。ユーザー名・自己紹介・アバターは本人がプロフィール設定で編集し、部署・役職・チームはテナント Admin / オーナーがメンバー設定（`PUT /v1/tenants/{id}/members/{user_id}`）で編集する（TASK-334）。

返すのは `id` / `username` / `bio` / `avatar_url` / `department` / `position` / `team` の 7 項目だけである。部署・役職・チームはこのテナントでの値で、未設定・ゲストでは null になる。メールアドレス・管理者フラグ・凍結フラグ・2FA の有効状態・`has_password` は返さない。自分自身を指定しても同じ形で返す。

## データ

このページで扱う `users` の列は次のとおりである。

| 列 | 説明 |
|---|---|
| `username` | ユーザー名。一意制約は無い |
| `bio` | 自己紹介。空文字が有効な値である |
| `avatar_url` | アバターの URL |
| `avatar_storage_key` | アップロードした画像のキー。URL を指定しているときは `NULL` |

`username` / `bio` / `avatar_url` の定義は [m20260520000000_initial_schema.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260520000000_initial_schema.rs)、`avatar_storage_key` は [m20261016020000_user_avatar_storage_key.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261016020000_user_avatar_storage_key.rs) にある。部署・役職・チームの列は [m20261021000000_tenant_members_org_profile.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20261021000000_tenant_members_org_profile.rs)、Device Token は [m20260924010000_device_tokens.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260924010000_device_tokens.rs) にある。

## API

| メソッド | パス | 説明 |
|---|---|---|
| PATCH | `/v1/auth/me` | 自分のプロフィールを更新し、`UserResponse` を返す |
| POST | `/v1/auth/me/avatar` | アバター画像をアップロードする |
| GET | `/v1/avatars/{key}` | アップロードしたアバター画像を配信する |
| GET | `/v1/personal_tokens` | 自分のトークンの一覧 |
| POST | `/v1/personal_tokens` | トークンを発行する |
| DELETE | `/v1/personal_tokens/{id}` | トークンを取り消す |
| GET | `/v1/users/me/devices` | 承認した端末の一覧 |
| DELETE | `/v1/users/me/devices/{id}` | 端末を失効させる |
| POST | `/v1/desktop/auth/codes` | Koyori Desktop の承認コードを発行する |
| GET | `/v1/tenants/{tenant_id}/users/{user_id}` | 他の利用者のプロフィール（`UserProfileResponse`）を返す |

`PATCH /v1/auth/me` の本文（`UpdateProfileRequest`）は次のとおりである。文字数は UTF-16 コードユニットではなく Unicode コードポイント単位で数える。`avatar_url` と `clear_avatar_url: true` は同時に指定できず、`400 Bad Request` を返す。

| フィールド | 型 | 制約 | 省略時 |
|---|---|---|---|
| `username` | `string?` | 3〜255 文字 | 変更しない |
| `bio` | `string?` | 1000 文字以内 | 変更しない |
| `avatar_url` | `string?` | 2048 文字以内、`https://` で始まる | 変更しない |
| `clear_avatar_url` | `bool` | | `false` |

`GET /v1/auth/me`（`UserResponse`）は `has_password: bool` を返す。ハッシュそのものは返さない。

## 認可

| 操作 | 誰が | スコープ |
|---|---|---|
| プロフィールの更新・アバターのアップロード | ログイン中の本人（`CurrentUser`） | セッション専用。下の表のとおり拒否する |
| アバターの配信 | 誰でも | 認証不要 |
| トークンの一覧・発行・取り消し | ログイン中の本人。発行はテナントオーナー限定（`require_tenant_owner`） | セッション専用。PAT は `403` |
| 端末の一覧・失効 | ログイン中の本人 | セッションと Device Token。PAT は `403`。Device Token で失効できるのはその端末自身だけで、別の端末を指定すると `403` |
| 他の利用者のプロフィール | テナントのオーナー・テナントメンバー | PAT は `read:tenant` を要求する（メンバー一覧と同じ） |

`GET /v1/auth/me` は Desktop の Device Token も通すが、更新は通さない。`CurrentUser` の拒否は次のとおりである。PAT で自分のプロフィールを書き換えられないことは、[個人アクセストークンの認可](/dev/design/personal-access-tokens-authz)の「アカウント API は PAT 非対応」に沿う。

| 状態 | 応答 |
|---|---|
| 未ログイン | `401` |
| `Authorization: Bearer` 付き（有効な PAT・Device Token） | `403`。アカウント API は Bearer 非対応 |
| 無効な Bearer（存在しない・失効・期限切れ） | `401` |
| 2FA の途中（half-authed） | `403` |
| 凍結済みの利用者 | `403` |
| 入力チェック違反 | `400` |

他の利用者のプロフィールの結果は次のとおりである。閲覧者の判定はテナントの所属判定（`ensure_tenant_access`）を通し、対象者の判定は `tenant_members` の行と[テナント / プロジェクト認可](/dev/design/tenant-project-authz)の既存関数（`is_tenant_owner` / `explicit_member_project_ids`）で行う。

| 状況 | 結果 |
|---|---|
| 閲覧者がテナントのオーナー・テナントメンバーで、対象がそのテナントのオーナー・メンバー・ゲスト | `200` |
| 閲覧者がテナントに入れない。ゲストと、テナントの 2FA 強制を満たさない場合を含む | `403` |
| 対象がそのテナントに関わっていない / 存在しない | `404` |

## 画面

### プロフィール

| 項目 | 値 |
|---|---|
| URL | `/settings/profile` |
| ページファイル | `apps/frontend/src/pages/settings/profile/+Page.vue` |
| 入口 | サイドバー下部の利用者メニューの「Account」 |

テナントに紐づかない個人の設定なので、テナントスコープ（`/{tenant}/...`）の外に置く。左のサイドバーには、ヘッダーのテナント切り替えで選んでいるテナントのナビ（ホーム・My Tasks・カレンダー・プロジェクト一覧）を出す。左にアカウント設定のナビ、右にプロフィールのフォームを並べる。

ナビは 6 項目を表示する。

- リンクとして機能するのは「プロフィール」「環境設定」「セキュリティ」「アクセストークン」の 4 項目である
- 「通知」「セッション」は `aria-disabled` の押せない項目として並べる
- 環境設定で「実験的機能を表示する」を有効にしたときだけ、区切り線の下に「実験的機能」（`/settings/experiments`）が加わる
- 項目は `apps/frontend/src/components/settings/AccountSettingsNav.vue` の `items` に定義する。`href` を足すとその項目が有効になる

環境設定（`/settings/preferences`）と実験的機能の値はブラウザの localStorage に保存する。アカウントには保存しないので、ほかの端末や別のブラウザには引き継がれない。環境設定の項目は「実験的機能を表示する」「絵文字の字」「タスク一覧でサブタスクを最初から展開する」の 3 つで、定義は `apps/frontend/src/lib/preferences.ts` の `PREFERENCE_TABLE` に置く。

画面上部のパンくずは実データから組む（[パンくず](/dev/frontend/breadcrumbs)）。設定系の画面は `プロフィール` のように一段で出し、`Account settings` に当たる中間の段は置かない。

フォームの挙動は次のとおりである。

- 初期値は `GET /v1/auth/me` の結果である
- アバターは URL 入力欄とプレビューである。「画像を選ぶ」ボタンで PNG / JPEG / GIF / WebP を選ぶと、その場で `POST /v1/auth/me/avatar` に送る。「変更を保存」は要らない
- 2 MiB を超える画像は送る前に止める
- アップロードに成功したら応答の `UserResponse` で `/v1/auth/me` のクエリと認証ストアを更新し、URL 欄も新しい URL にする。失敗は `413`（サイズ）/ `400`（形式）/ それ以外で文言を分けて出す
- 「削除」ボタンは入力欄を空にする。この時点では保存せず、保存すると `clear_avatar_url` を送る
- 保存済みの値はスキームを問わずプレビューする。`APP_BASE_URL` が http の開発環境で、アップロードした画像を表示するためである
- 保存すると `PATCH /v1/auth/me` を送り、成功したら `/v1/auth/me` のクエリを無効化してサイドバーの表示名・アバターにも反映する
- 入力チェックは送信前（`onSubmit`）と項目からフォーカスが外れたとき（`onBlur`）の両方で走り、条件は API 側と同じものを持たせる

### アクセストークン

| 項目 | 値 |
|---|---|
| URL | `/settings/tokens` |
| ページファイル | `apps/frontend/src/pages/settings/tokens/+Page.vue` |
| 本体 | `apps/frontend/src/components/settings/AccessTokensSection.vue` |

- 一覧の各行に、トークン名、末尾 4 文字だけの伏せ字（`pat_••••••7f3a`）、スコープ数、有効期限、最終使用日時を表示する。スコープの内訳は「スコープを表示」のドロップダウンで開く
- 発行は「トークンを発行」でフォームを開き、`POST /v1/personal_tokens` を送る
- 平文トークンは発行後にコピー付きで 1 度だけ表示し、再表示できないことを明記する
- 取り消しは確認ダイアログを経て `DELETE /v1/personal_tokens/{id}` を送る
- オーナーのテナントが無い利用者には発行ボタンを無効にし、その旨を表示する。`403` が返った場合もオーナー限定であることを伝える

発行フォームの項目は次のとおりである。

| 項目 | 内容 |
|---|---|
| トークン名 | 1〜100 文字 |
| テナント | 自分がオーナーのテナントのみ。1 件ならフォームに出さず自動選択する |
| 有効期限プリセット | 30日 / 90日（既定）/ 1年 / 無期限 |
| スコープ | backend の全 16 種。1 つ以上必須で、説明付きのチェックボックスである |
| プロジェクトの絞り込み（`project_ids`） | 常に `null`（テナント内の全プロジェクト）で送る |

### セキュリティ

パスワードと OAuth 連携を「サインインできる方法」としてまとめて管理する画面である。OAuth の認可フローと解除規則の定義元は [OAuth ログイン](/dev/features/auth/oauth)である。

| 項目 | 値 |
|---|---|
| URL | `/settings/security` |
| ページファイル | `apps/frontend/src/pages/settings/security/+Page.vue` |
| 本体 | `apps/frontend/src/components/settings/AuthMethodsSection.vue` |
| パスワード行 | `apps/frontend/src/components/settings/PasswordMethodRow.vue` |

1 枚のカードに「パスワード」「連携済み」「追加できる連携」を上から並べる。

パスワードの行は `GET /v1/auth/me` の `has_password` で出し分ける。

| 状態 | ボタン | 送信先 | 入力 |
|---|---|---|---|
| 設定済み | パスワードを変更 | `POST /v1/auth/password/change` | 現在 + 新しい + 確認 |
| 未設定（OAuth のみ） | パスワードを設定 | `POST /v1/auth/password` | 新しい + 確認 |

変更はすべてのセッションと PAT を失効させるので、フォームを開いた時点でその旨を出す。成功したら `/signin` へフルページ遷移し、サインイン画面はなぜサインアウトされたのかを表示する。

入力チェックは `lib/auth-methods.ts` の `validatePasswordForm` である。8 文字以上、確認との一致に加え、変更のときだけ「現在のパスワードが空でない」「現在と同じ値でない」を見る。強度表示はサインアップと同じ `PasswordStrengthBar` を使う。

OAuth 連携は次のとおりである。

- 連携済みは `GET /v1/auth/oauth/connections` で取り、プロバイダー名、`provider_email`、接続日時、self-hosted の `instance_url` を出す
- 解除は確認を挟んで `DELETE /v1/auth/oauth/connections/{provider}` を送る。self-hosted は `instance_url` をクエリに添える
- 追加できる連携は `GET /v1/auth/oauth/providers` のうち未連携のものである。連携済みかどうかは開始用 slug ではなく `connection_provider`（連携一覧が返す識別子）で突き合わせる
- `requires_instance_url` のプロバイダー（GitLab セルフホスト）は、インスタンスが違えば別の連携として足せるので、1 件連携済みでも候補に残す。連携済みと同じインスタンス URL を入れた場合は開始しない。backend も同じインスタンスの別アカウントの接続を `409` で弾き、同じアカウントの接続し直しはトークンの更新になる
- 「連携する」で既存の OAuth 開始 URL へフルページ遷移する。`redirect_after` にはこの画面のパスだけを渡す。プロバイダー側のエラーは backend が `?oauth_error=` を付けて同じ画面に戻す
- 開始 URL の組み立てとプロバイダー表示名は、サインイン画面の `OAuthButtons` と共通の `lib/oauth-providers.ts` に置く

「連携しました」「パスワードを変更しました」は、URL のクエリではなく `lib/one-time-notice.ts` のマーカー（sessionStorage）を根拠に出す。マーカーは操作を始める側が置き、戻ってきた画面が 1 回だけ消費する。連携についてはさらに、`GET /v1/auth/oauth/connections` にその接続が実際に入っていることまで確かめてから出す。マーカーにはプロバイダー名だけでなく `connection_provider` とインスタンス URL も入れて、開始したその接続が入ったことを確かめる。

backend は「その連携がログイン用の最後の 1 件 かつ パスワード無し かつ パスキー無し」のとき解除を `403 oauth-last-auth-method` で拒む。PR 承認用の連携（`sign_in: false`）は数えない。画面はこの判定を先取りして注意書きを出すだけで、可否はサーバーの応答に従う。先取りの数え方（`countAuthMethods`）は backend と揃えてパスキーも数えるので、この画面は件数を見る目的で `GET /v1/auth/passkeys` も呼ぶ。

認証方法の下に、承認した Koyori Desktop（Device Token）の一覧と失効を出す（`apps/frontend/src/components/settings/DevicesSection.vue`）。各行に端末名、末尾 4 文字だけの伏せ字、有効期限、最終使用を出し、失効は確認ダイアログを経る。

### Koyori Desktop の承認

承認は `/desktop/authorize`（`apps/frontend/src/pages/desktop/authorize/+Page.vue`）で行う。

- Desktop が開いたクエリ（`port` / `code_challenge` / `state` / `name`）を `lib/desktop-authorize.ts` で検証する。`port` は 1024〜65535 の整数のみである
- 承認で `POST /v1/desktop/auth/codes` を呼んで `http://127.0.0.1:{port}/callback?code=&state=` へ遷移する
- 未ログインなら戻り先を `lib/one-time-notice.ts` のマーカーに置いてサインインへ送り、パスワードでのサインイン後にこの画面へ戻す
- OAuth でのサインインは戻らないので、Desktop からやり直す

### 他の利用者のプロフィール

| 項目 | 値 |
|---|---|
| URL | `/{tenant}/users/{user_id}` |
| ページ | `apps/frontend/src/pages/@tenant/users/@userId/+Page.vue` |
| 入口 | タスク詳細の担当者名・コメントの投稿者・テナント設定のメンバー一覧の名前 |

部署・役職・チームは値のあるものだけを username の下に出す。自分のときだけ「プロフィールを編集」を出す。

## 未実装と既知の制限

モックアップにあるが、実装していない項目は次のとおりである。

| 項目 | 理由 | 必要な作業 |
|---|---|---|
| 表示名（Display name） | `users` に列が無い | マイグレーションで列を追加し、`UserResponse` と `UpdateProfileRequest` に載せる |
| タイムゾーン | `users` に列が無い | 同上。加えて日時表示側の適用箇所を決める必要がある |
| 言語 | `users` に列が無い | 同上。i18n の仕組み自体がまだ無い |
| メールアドレスの変更 | 変更後の再確認の流れが無い（`email_verification` は新規登録用） | 変更要求、新アドレスへの確認メール、確認後の反映の経路を新設する |
| 通知 / セッションの各画面 | 画面が無い | 通知は `/v1/users/me/notification-settings/{project_id}` が既にある |
| 二要素認証・パスキーの管理 | 認証方法とは別タスク（TASK-188 / TASK-189）。[2 段階認証の未実装](/dev/features/auth/2fa#未実装と既知の制限)、[パスキーの未実装](/dev/features/auth/passkeys#未実装と既知の制限) | セキュリティ画面にセクションを足す。API は既にある |
| トークンのプロジェクト絞り込み | アクセストークン画面の対象外 | API（`project_ids`）は既にある。フォームに選択 UI を足す |

ほかの制限は次のとおりである。

- `users.username` の一意制約は入れていない
- アップロード画像の片付けに失敗すると、孤児の画像がストレージに残る

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-08-19 | `/settings/profile` をテナントスコープの外に置く | テナントに紐づかない個人の設定である |
| 2026-08-19 | `users.username` に一意制約を入れず、同名を許す | 記録なし |
| 2026-08-19 | `bio` に `clear_bio` を用意しない | `bio` は空文字がそのまま有効な値で、新規登録時も空文字が入るので、消去は `{"bio": ""}` で足りる |
| 2026-08-19 | `avatar_url` の消去は `clear_avatar_url` のフラグで行う | `avatar_url` は `https://` で始まることを要求するので空文字は入力チェックで弾かれる。既存の `PATCH .../tasks/{id}`（`UpdateTaskRequest`）が nullable な項目に `clear_*` を持たせているのと同じ形にする |
| 2026-08-24 | `avatar_url` は `https://` だけを受け付ける | `javascript:` や `data:` を保存できるとそのまま描画経路に載る。HTTP は mixed content で表示できない |
| 2026-09-05 | パスワードの変更後は `/signin` へフルページ遷移する | クライアントルーティングにすると、失効済みのセッションで取ったキャッシュを持ち越す |
| 2026-09-05 | 最後の認証方法の注意書きは先取りに留め、可否はサーバーの応答に従う | 画面が数えた後に別のタブで増減されうる |
| 2026-09-05 | 開始 URL とプロバイダー表示名を `lib/oauth-providers.ts` で共有する | 片方だけ直すと画面ごとに名前や戻り先が食い違う |
| 2026-09-07 | 成功通知はクエリではなく sessionStorage のマーカーを根拠にする | クエリだけで判定すると、その URL を開かせるだけで、連携していない人に「連携しました」を、変更していない人に「失効しました」を読ませられる |
| 2026-09-07 | 連携の成功通知は連携一覧に接続が入っていることまで確かめる | 承認の途中で失敗してこの画面に戻らなかったとき、マーカーだけが残って次の来訪で誤って出るのを防ぐ |
| 2026-09-07 | マーカーに `connection_provider` とインスタンス URL も入れる | プロバイダー名だけで比べると、GitLab セルフホストのインスタンス A を連携済みのまま B の承認を中断した人に、A を見て「連携しました」が出る |
| 2026-09-07 | 連携済みの判定は `connection_provider` で突き合わせる | 汎用 OIDC は開始用 slug が `oidc`、連携一覧の識別子が `oidc:{issuer}` で形が違い、slug で比べると連携済みでも候補に残り続ける |
| 2026-09-10 | パンくずに `Account settings` の中間の段を置かない | その名で開ける画面が無い |
| 2026-09-24 | Desktop の承認の戻り先は認証ガードに任せず、マーカーに置く | 認証ガードに任せるとクエリが落ちる |
| 2026-10-15 | 対象がテナントに関わらない場合を `403` にせず `404` にする | `users` はテナント横断の表で、`403` と `404` を分けると任意の UUID について利用者が実在するかを調べられる |
| 2026-10-16 | アバターの画像を消す専用の API を設けない | `clear_avatar_url` で足りる |
| 2026-10-16 | 本文全体の上限を multipart の分だけ 64 KiB 広げる | 上限ちょうどの画像を multipart の境界とヘッダーのせいで落とさない |
| 2026-10-16 | 古い画像はコミットの後に消し、失敗しても応答は成功のままにする | 孤児の画像が残るだけで、利用者から見える状態は正しい |
| 2026-10-17 | メンションの宛先は username でなく利用者 ID で決める | 同名の利用者が同じプロジェクトにいても補完で選んだ一人にだけ届き、改名してもマーカーが切れない |
| 2026-11-09 | タスク一覧のサブタスクの初期展開は環境設定（localStorage）に置き、URL やプロジェクト設定にしない | 共有した相手の見え方を変える値ではなく、読む人の好みなので端末ごとでよい |
