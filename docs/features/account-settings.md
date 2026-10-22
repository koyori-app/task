---
title: アカウント設定・プロフィール編集
description: ログイン中ユーザーが自分のプロフィール・アクセストークンを管理する画面と API
icon: lucide:user-cog
---

# アカウント設定・プロフィール編集 仕様書

> ステータス: **Draft** / 作成日: 2026-08-19
> 依存: `users` テーブル（アバターのアップロードで `avatar_storage_key` 列を追加）、ストレージバックエンド（[ドライブ](./drive.md) §6）

---

## 1. 概要

ログイン中のユーザーが、自分のプロフィールを自分で編集できるようにする。

これまで `users` の内容を変更できる API は管理者専用の `PATCH /v1/admin/users/{id}` だけで、
本人がユーザー名や自己紹介を直す手段が無かった。本仕様で `PATCH /v1/auth/me` を追加し、
`/settings/profile` から編集できるようにする。

編集できるのは `users` の列のうち、本人が持ち主である 3 つに限る。

| 項目 | 列 | 備考 |
|---|---|---|
| ユーザー名 | `users.username` | 一意制約は無い（同名を許す）。後述の影響あり |
| 自己紹介 | `users.bio` | |
| アバター | `users.avatar_url` | 画像をアップロードする（`POST /v1/auth/me/avatar`）か、画像の URL を指定する。アップロードした画像のキーは `users.avatar_storage_key` |

メールアドレス・管理者フラグ・凍結フラグ・2FA の有効状態はこの API では変更できない。

### 同名を許すことの影響

`users.username` に一意制約は無く、この API も重複を確認しない。したがって、既存の利用者と
同じ `username` に変更できる。登録 API（`POST /v1/auth/register`）も同様に重複を許す。
本人によるプロフィール更新は監査ログに記録し、ユーザー名の変更前後を追跡できる。

メンションの宛先は username の一致では決めない。コメント・タスクの説明文のメンションは `<@利用者ID>` の印で
利用者の ID を指す（`docs/features/tasks/4.collaboration.md` §3）ので、同名の利用者が同じ
プロジェクトにいても、補完で選んだ一人にだけ届く。改名しても印は切れない。
ただし画面上では、コメント・担当者・`AvatarGroup` の表示名が同じになる。
`users.username` の一意制約は、今も入れていない。

---

## 2. 画面

### URL と入口

| 項目 | 値 |
|---|---|
| URL | `/settings/profile` |
| ページファイル | `apps/frontend/src/pages/settings/profile/+Page.vue` |
| 入口 | サイドバー下部のユーザーメニュー → **Account** |

テナントに紐づかない個人の設定なので、テナントスコープ（`/{tenant}/...`）の外に置く。

### 構成

左にアカウント設定のナビ、右にプロフィールのフォームを並べる。

ナビは 6 項目を表示するが、リンクとして機能するのは
**プロフィール** / **セキュリティ** / **アクセストークン**。
残りの 3 項目（環境設定 / 通知 / セッション）は、
対応する画面がまだ無いため `aria-disabled` の押せない項目として並べる。

> ナビ項目は `apps/frontend/src/components/settings/AccountSettingsNav.vue` の `items` に定義する。
> 画面を実装したら、その項目に `href` を足すだけで有効になる。

### フォームの挙動

- 初期値は `GET /v1/auth/me` の結果。
- アバターは URL 入力欄とプレビュー。**画像を選ぶ**ボタンで PNG / JPEG / GIF / WebP を選ぶと、
  その場で `POST /v1/auth/me/avatar` に送る（「変更を保存」は不要）。2 MiB を超える画像は送る前に止める。
  成功したら応答の `UserResponse` で `/v1/auth/me` のクエリと認証ストアを更新し、URL 欄も新しい URL にする。
  失敗は 413（サイズ）/ 400（形式）/ それ以外で文言を分けて出す。
- **削除**ボタンは入力欄を空にする（この時点では保存しない）。保存すると `clear_avatar_url` を送る。
- 保存済みの値はスキームを問わずプレビューする（`APP_BASE_URL` が http の開発環境で、アップロードした画像を表示するため）。
- 保存すると `PATCH /v1/auth/me` を送り、成功したら `/v1/auth/me` のクエリを無効化して
  サイドバーの表示名・アバターにも反映する。
- 入力チェックは送信前（`onSubmit`）と項目からフォーカスが外れたとき（`onBlur`）の両方で走る。
  条件は後述の API 側と同じものを持たせているので、正常な入力が 400 で跳ね返ることはない。

---

## 3. API

### `PATCH /v1/auth/me`

ログイン中ユーザー自身のプロフィールを更新する。

**リクエスト**（`UpdateProfileRequest`）

| フィールド | 型 | 制約 | 省略時 |
|---|---|---|---|
| `username` | `string?` | 3〜255 文字 | 変更しない |
| `bio` | `string?` | 1000 文字以内 | 変更しない |
| `avatar_url` | `string?` | 2048 文字以内、`https://` で始まる | 変更しない |
| `clear_avatar_url` | `bool` | | `false` |

文字数は UTF-16 コードユニットではなく Unicode コードポイント単位で数える。
`avatar_url` と `clear_avatar_url: true` は同時に指定できず、競合時は `400 Bad Request` を返す。

**レスポンス**: `200 OK` / `UserResponse`（更新後の値）

### 省略と消去の区別

送らなかったフィールドは変更しない。フィールドが 1 つも無い `{}` を送った場合も
`200 OK` を返し、値は変わらない（空の `UPDATE` は発行しない）。

`avatar_url` を消したいときだけ、値ではなくフラグで指定する。

```json
{ "clear_avatar_url": true }
```

空文字（`""`）では消せない。`avatar_url` は `https://` で始まることを要求するため、
空文字は入力チェックで弾かれるからである。既存の `PATCH .../tasks/{id}`（`UpdateTaskRequest`）
が nullable な項目に `clear_*` を持たせているのと同じ形にした。

`bio` には `clear_bio` を用意しない。`bio` は空文字がそのまま有効な値で、
新規登録時も空文字が入る（`NULL` と `""` をアプリ側で区別していない）ため、
消去は `{"bio": ""}` で足りる。

### 認可

エクストラクタはセッション専用の `CurrentUser` を使う（`GET /v1/auth/me` は Desktop の Device Token も通すが、更新は通さない）。したがって次の拒否をそのまま引き継ぐ。

| 状態 | 応答 |
|---|---|
| 未ログイン | `401` |
| `Authorization: Bearer` 付き（PAT） | `401` — アカウント API は PAT 非対応 |
| 2FA の途中（half-authed） | `403` |
| 凍結済みユーザー | `403` |
| 入力チェック違反 | `400` |

PAT で自分のプロフィールを書き換えられないことは、
[個人アクセストークンの認可](../../apps/backend/docs/personal-access-tokens-authz.md) の
「アカウント API は PAT 非対応」に沿う。

### `avatar_url` のスキーム制限

`avatar_url` はフロントエンドで `<img src>` に流し込む。`javascript:` や `data:` を保存できると
そのまま描画経路に載るため、`https://` 以外を保存段階で弾く
（`payload::users::validate_avatar_url`）。HTTP は mixed content で表示できないため、相対パスと同様に受け付けない。

同じ判定をフロントエンドの `ProfileForm.vue` にも置いている。**片方だけ変えると、
画面では通るのに保存で 400 になる**（またはその逆）ので、変更するときは両方を直すこと。
前後の空白は、画面側の判定・送信値のどちらでも取り除く。

なお `avatar_url` は本人以外の画面でも `<img src>` に載る（担当者一覧の `AvatarGroup` など）。
指定された URL は閲覧者のブラウザが直接取りに行くため、その URL を用意した側は
アバターを見た人の IP アドレスと User-Agent を知り得る。
アップロードしたアバターは自サーバーから配信するので IP の問題はない。外部 URL を指定した場合は従来どおり。

アップロードで入る `avatar_url`（`{APP_BASE_URL}/v1/avatars/...`）は、`APP_BASE_URL` が http の開発環境では
`https://` で始まらない。サーバーが書き込む値なのでこの制限は掛からず、画面側も現在値は検証しない。

### `POST /v1/auth/me/avatar`

ログイン中ユーザーのアバター画像をアップロードし、`avatar_url` を自サーバーの配信 URL に差し替える。

- **認証**: `PATCH /v1/auth/me` と同じ `CurrentUser`（§3「認可」の表がそのまま当てはまる。PAT は 401）
- **リクエスト**: `multipart/form-data`。フィールド `file` を 1 つ
- **サイズ**: 画像は 2 MiB（`service::avatar::AVATAR_MAX_BYTES`）まで。超えたら `413`。
  本文全体の上限（`RequestBodyLimitLayer`）はこのルートだけに掛け、multipart の枠の分だけ 64 KiB 広げてある
  （上限ちょうどの画像を枠のせいで落とさないため）。本文はメモリに読む
- **形式**: クライアントの content-type は信用せず、先頭バイトで判定する
  （`service::avatar::detect_image_format`）。それ以外・空は `400`（`unsupported-avatar-format`）、`file` が無ければ `400`

  | 形式 | 先頭バイト | 拡張子 |
  |---|---|---|
  | PNG | `89 50 4E 47` | `png` |
  | JPEG | `FF D8 FF` | `jpg` |
  | GIF | `GIF8` | `gif` |
  | WebP | `RIFF????WEBP` | `webp` |

- **保存**: ドライブと同じストレージバックエンドに `avatar-{uuid v4}.{拡張子}` のキーで置く（`drive_files` の行は作らない）。
  リサイズ・寸法の検査・EXIF の除去はしない
- **DB 更新**: トランザクションで `avatar_url = {APP_BASE_URL}/v1/avatars/{key}`、`avatar_storage_key = key` にし、
  監査ログ `user.profile.update`（`changed_fields: ["avatar_url"]`、`PATCH /v1/auth/me` と同じ形）を残す。
  DB 更新に失敗したら、置いた画像を消してからエラーを返す
- **レスポンス**: `200 OK` / `UserResponse`

`avatar_url` の生成に使う `APP_BASE_URL` は OAuth のコールバック URL と同じ設定値で、ブラウザから届く
バックエンドの公開 URL にする。

### `GET /v1/avatars/{key}`

アップロードしたアバター画像を配信する。

- **認証なし**。`<img src>` から取れる必要があり、Koyori Desktop も同じ URL を使う。キーは UUID v4 で推測できない
- `key` が `^avatar-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(png|jpg|gif|webp)$` に
  一致しなければ `404`。DB は引かず、`Content-Type` は拡張子から決める。画像が無ければ `404`
- ヘッダ: `Content-Type`、`Cache-Control: public, max-age=31536000, immutable`、`Content-Disposition: inline`
  （`X-Content-Type-Options: nosniff` は全経路共通の層で付く）。差し替えるとキーごと変わるので、
  同じ URL の中身は変わらない

### アップロードした画像の片付け

使われなくなった画像は、DB の更新をコミットした**後**に消す。消すのに失敗しても応答は成功のままにし、
`tracing::warn!` で記録する（孤児の画像が残るだけで、利用者から見える状態は正しい）。

| 契機 | 消す画像 |
|---|---|
| 再アップロード | 直前の `avatar_storage_key` |
| `PATCH /v1/auth/me` で `clear_avatar_url: true`、または `avatar_url` を別の URL に変更 | 直前の `avatar_storage_key`（列は `NULL` に戻す） |
| 管理者によるユーザー削除（`DELETE /v1/admin/users/{id}`） | 対象ユーザーの `avatar_storage_key` |

アバター以外の項目だけを変える `PATCH` では消さない。専用の削除 API は設けない（`clear_avatar_url` で足りる）。

---

## 4. 未対応

モックアップにあるが、この仕様では実装していない項目と、その理由。

| 項目 | 未対応の理由 | 必要な作業 |
|---|---|---|
| 表示名（Display name） | `users` に列が無い | マイグレーションで列を追加し、`UserResponse` と `UpdateProfileRequest` に載せる |
| タイムゾーン | 同上 | 同上。加えて日時表示側の適用箇所を決める必要がある |
| 言語 | 同上 | 同上。i18n の仕組み自体がまだ無い |
| メールアドレスの変更 | 変更後の再確認フローが無い（`email_verification` は新規登録用） | 変更要求 → 新アドレスへ確認メール → 確認後に反映、の経路を新設する |
| 環境設定 / 通知 / セッション の各画面 | 本仕様の対象外 | 通知は `/v1/users/me/notification-settings/{project_id}` が既にある |
| 二要素認証・パスキーの管理 | 認証方法（§7）とは別タスク（TASK-188 / TASK-189） | セキュリティ画面にセクションを足す。API は既にある |
| トークンのプロジェクト絞り込み | アクセストークン画面（§6）の対象外 | API（`project_ids`）は既にある。フォームに選択 UI を足す |

画面上部のパンくずは実データから組まれるようになった（[breadcrumbs.md](../breadcrumbs.md)）。
設定系の画面は `プロフィール` のように一段で出る。`Account settings` に当たる中間の段は、
その名で開ける画面が無いため置いていない。

---

## 5. テスト

| 種別 | ファイル | 内容 |
|---|---|---|
| 統合 | `apps/backend/tests/profile_update_integration.rs` | 更新と永続化、省略項目を壊さないこと、`clear_avatar_url`、空 PATCH、スキーム拒否、長さ制限、未ログイン拒否、Bearer 拒否 |
| 統合 | `apps/backend/tests/personal_tokens_list_integration.rs` | 一覧が自分の有効なトークンだけを名前順で返すこと、平文・ハッシュを含まないこと、取り消しで一覧から消えること、空一覧、未ログイン拒否、Bearer 拒否 |
| 統合 | `apps/backend/tests/integration/avatar_upload_integration.rs` | アップロードで `avatar_url` と `avatar_storage_key` が入ること、認証なしの配信（`Content-Type` / `Cache-Control`）、2 MiB ちょうどは通り超えると 413、画像でない・空のバイト列の 400、再アップロード・`clear_avatar_url`・外部 URL への変更・管理者のユーザー削除で旧画像が 404 になること（アバター以外の更新では残る対照込み）、Bearer 拒否、不正なキー形式・存在しないキーの 404 |
| 単体 | `apps/backend/crates/service/src/avatar.rs` | 先頭バイトでの形式判定（4 形式・不正・空・途中で切れた RIFF）、配信キーの形式 |
| 単体 | `apps/frontend/src/components/settings/__tests__/ProfileForm.test.ts` | 空欄が `clear_avatar_url` になること、URL の送信、前後の空白を取り除くこと、不正スキームとユーザー名長を送信前に止めること、失敗時（400 / それ以外）の表示、保存後に編集を再開すると保存済み表示が消えること、画像アップロードの成功で表示・URL 欄・認証ストアが更新されること、その後の保存で `avatar_url` を送り直さないこと、2 MiB 超を送る前に止めること（ちょうどは送る）、413 / 400 / それ以外の表示 |
| 単体 | `apps/frontend/src/components/settings/__tests__/AccessTokensSection.test.ts` | 一覧表示（伏せ字・スコープ数・期限・最終使用）、発行リクエストの内容（90 日既定・無期限）、名前とスコープの入力チェック、403 の表示、取り消しの確認ダイアログと失敗時の表示、オーナーのテナントが無い場合の無効化 |
| 単体 | `apps/frontend/src/lib/__tests__/personal-tokens.test.ts` | 有効期限プリセットの計算、伏せ字、期限・最終使用の表示文言（境界値込み） |
| 統合 | `apps/backend/tests/oauth_integration.rs` | `has_password` の反転、初回パスワード設定が一度きりであること、パスワードを得たあとに最後の連携を解除できること |
| 単体 | `apps/frontend/src/components/settings/__tests__/AuthMethodsSection.test.ts` | パスワードあり／なしの出し分け、初回設定に現在のパスワード欄を出さないこと、確認不一致で送信しないこと、現在のパスワード不一致の表示、変更後のサインイン画面への遷移、連携一覧と追加候補、self-hosted の URL 未入力での無効化、連携済みでも別インスタンスを開始できること、同一インスタンスを開始しないこと、成功通知が印を根拠にすること（URL だけでは出ないこと）、確認を挟む解除と `instance_url` の付与、最後の認証方法の拒否表示 |
| 単体 | `apps/frontend/src/lib/__tests__/auth-methods.test.ts` | 認証方法の数え方（パスキー込み）、パスワード入力チェック（8 文字の境界・現在と同値・確認不一致） |
| 統合 | `apps/backend/tests/integration/user_profile_integration.rs` | 他人のプロフィール（§8）: 返す項目が 4 つだけでメールを含まないこと、オーナー・客分・自分を対象に開けること、テナントに関わらない人と存在しない UUID が同じ 404 になること、テナント外の人と客分の閲覧が 403 になること、PAT の `read:tenant` と `read:task` の対照、未ログイン拒否 |
| 単体 | `apps/frontend/src/components/tasks/__tests__/TaskComments.test.ts` | `profileHref` を渡したときだけ投稿者がプロフィールへのリンクになること |

拒否を確認するテストには、対照として通るケースも置いている（過剰に拒否していないことの確認）。

---

## 6. アクセストークン画面

パーソナルアクセストークン（PAT）を発行・取り消しする画面。PAT 自体の認可設計は
[個人アクセストークンの認可](../../apps/backend/docs/personal-access-tokens-authz.md) を正とする。

| 項目 | 値 |
|---|---|
| URL | `/settings/tokens` |
| ページファイル | `apps/frontend/src/pages/settings/tokens/+Page.vue` |
| 本体 | `apps/frontend/src/components/settings/AccessTokensSection.vue` |

### 画面の構成と挙動

- **一覧**: `GET /v1/personal_tokens`（この画面のために新設）。自分が発行した
  取り消し前のトークンを名前の昇順で返す。各行にトークン名、末尾 4 文字だけの伏せ字
  （`pat_••••••7f3a`）、スコープ数、有効期限、最終使用日時を表示する。
  スコープの内訳は「スコープを表示」のドロップダウンで開く。
- **発行**: 「トークンを発行」でフォームを開き、`POST /v1/personal_tokens` を送る。
  - トークン名（1〜100 文字）
  - テナント（自分が**オーナー**のテナントのみ。1 件ならフォームに出さず自動選択）
  - 有効期限プリセット: 30日 / 90日（既定）/ 1年 / 無期限
  - スコープ（backend の全 11 種。1 つ以上必須。説明付きチェックボックス）
  - プロジェクトの絞り込み（`project_ids`）は常に `null`（テナント内全プロジェクト）で送る
- **平文トークンは発行直後の応答でしか取得できない**。発行後にコピー付きで 1 度だけ表示し、
  再表示できないことを明記する。
- **取り消し**: 確認ダイアログを経て `DELETE /v1/personal_tokens/{id}`。取り消したトークンは
  一覧に出なくなる（行の削除ではなく `revoked` フラグ。API は冪等）。

### 認可まわりの注意

- PAT の発行はテナントオーナー限定（backend の `require_tenant_owner`）。オーナーの
  テナントが無いユーザーには発行ボタンを無効にし、その旨を表示する。403 が返った場合も
  オーナー限定であることを伝える。
- トークン管理 API はすべてセッション専用。PAT（Bearer）でこの画面の API は呼べない。

### `GET /v1/personal_tokens`

この仕様で新設した一覧 API。

- 認可: セッション必須（PAT は 403）
- 返すもの: 自分（`user_id`）のトークンのうち `revoked = false` のもの。
  名前の昇順（同名は `id` 順）。`PersonalTokenResponse` の配列で、平文トークン・ハッシュは含まない
- トークンを 1 件も持たない場合は空配列（404 にしない）

---

## 7. セキュリティ画面（認証方法）

パスワードと OAuth 連携を「サインインできる方法」としてまとめて管理する画面。
OAuth の認可フローと解除規則は [OAuth ログイン](/features/auth-oauth) を正とする。

| 項目 | 値 |
|---|---|
| URL | `/settings/security` |
| ページファイル | `apps/frontend/src/pages/settings/security/+Page.vue` |
| 本体 | `apps/frontend/src/components/settings/AuthMethodsSection.vue` |
| パスワード行 | `apps/frontend/src/components/settings/PasswordMethodRow.vue` |

1 枚のカードに「パスワード」「連携済み」「追加できる連携」を上から並べる。

### パスワード

`GET /v1/auth/me` の `has_password` で出し分ける。

| 状態 | ボタン | 送信先 | 入力 |
|---|---|---|---|
| 設定済み | パスワードを変更 | `POST /v1/auth/password/change` | 現在 + 新しい + 確認 |
| 未設定（OAuth のみ） | パスワードを設定 | `POST /v1/auth/password` | 新しい + 確認 |

変更はすべてのセッションと PAT を失効させるため、フォームを開いた時点でその旨を出す。
成功したら `/signin` へフルページ遷移し、サインイン画面はなぜサインアウトされたのかを表示する。
クライアントルーティングにしないのは、失効済みのセッションで取ったキャッシュを
持ち越さないため。

入力チェックは `lib/auth-methods.ts` の `validatePasswordForm`。8 文字以上、確認との一致、
変更のときだけ「現在のパスワードが空でない」「現在と同じ値でない」。強度表示は
サインアップと同じ `PasswordStrengthBar` を使う。

### OAuth 連携

- **連携済み**: `GET /v1/auth/oauth/connections`。プロバイダー名、`provider_email`、接続日時、
  self-hosted の `instance_url` を出す。解除は確認を挟んで
  `DELETE /v1/auth/oauth/connections/{provider}`（self-hosted は `instance_url` をクエリに添える）
- **追加できる連携**: `GET /v1/auth/oauth/providers` のうち未連携のもの。連携済みかどうかは
  開始用 slug ではなく `connection_provider`（連携一覧が返す識別子）で突き合わせる。
  汎用 OIDC は開始用 slug が `oidc`、連携一覧の識別子が `oidc:{issuer}` で形が違い、
  slug で比べると連携済みでも候補に残り続ける。ただし
  `requires_instance_url` のプロバイダー（GitLab セルフホスト）は、インスタンスが違えば
  別の連携として足せるので、1 件連携済みでも候補に残す。連携済みと同じインスタンス URL を
  入れた場合は開始しない（backend も `(provider, instance_url)` の重複を 409 で弾く）。
  「連携する」で既存の OAuth 開始 URL へフルページ遷移する。`redirect_after` にはこの画面の
  パスだけを渡す。プロバイダー側のエラーは backend が `?oauth_error=` を付けて同じ画面に戻す

開始 URL の組み立てとプロバイダー表示名は、サインイン画面の `OAuthButtons` と共通の
`lib/oauth-providers.ts` に置く。片方だけ直すと画面ごとに名前や戻り先が食い違うため。

### 成功通知の根拠

「連携しました」「パスワードを変更しました」は、URL のクエリではなく `lib/one-time-notice.ts`
の印（sessionStorage）を根拠に出す。クエリだけで判定すると、その URL を開かせるだけで
連携していない人に「連携しました」を、変更していない人に「失効しました」を読ませられる。
印は操作を始める側が置き、戻ってきた画面が 1 回だけ消費する。連携についてはさらに、
`GET /v1/auth/oauth/connections` にその接続が実際に入っていることまで確かめてから出す
（承認の途中で失敗してこの画面に戻らなかったとき、印だけが残って次の来訪で誤って出るのを防ぐ）。

印にはプロバイダー名だけでなく `connection_provider` とインスタンス URL も入れて、
開始したその接続が入ったことを確かめる。プロバイダー名だけで比べると、GitLab セルフホストの
インスタンス A を連携済みのまま B の承認を中断した人に、A を見て「連携しました」が出る。

### 最後の認証方法

利用できる認証方法が 0 件にならないよう、backend は「その連携が最後の 1 件 かつ
パスワード無し かつ パスキー無し」のとき解除を `403 oauth-last-auth-method` で拒む。

画面はこの判定を先取りして注意書きを出すだけで、可否はサーバーの応答に従う
（画面が数えた後に別のタブで増減されうるため）。先取りの数え方（`countAuthMethods`）は
backend と揃えてパスキーも数えるので、この画面は件数を見る目的で `GET /v1/auth/passkeys` も呼ぶ。

### Koyori Desktop の端末

認証方法の下に、承認した Koyori Desktop（Device Token）の一覧と失効を出す
（`apps/frontend/src/components/settings/DevicesSection.vue`）。Device Token の規則は
[Bearer 認証・認可の「Desktop 認証」](../../apps/backend/docs/personal-access-tokens-authz.md) を正とする。

- **一覧**: `GET /v1/users/me/devices`。失効済み・期限切れを除いた端末を新しい順に返す。
  各行に端末名、末尾 4 文字だけの伏せ字、有効期限、最終使用を出す
- **失効**: 確認ダイアログを経て `DELETE /v1/users/me/devices/{id}`。`revoked_at` を立てるだけで
  行は残り、一覧からは消える。その端末は次の要求から 401 になる

承認そのものは `/desktop/authorize`（`apps/frontend/src/pages/desktop/authorize/+Page.vue`）で行う。
Desktop が開いたクエリ（`port` / `code_challenge` / `state` / `name`）を `lib/desktop-authorize.ts` で
検証し（`port` は 1024〜65535 の整数のみ）、承認で `POST /v1/desktop/auth/codes` を呼んで
`http://127.0.0.1:{port}/callback?code=&state=` へ遷移する。未ログインなら戻り先を
`lib/one-time-notice.ts` の印に置いてサインインへ送り、パスワードでのサインイン後にこの画面へ戻す
（認証ガードに任せるとクエリが落ちる）。OAuth でのサインインは戻らないので、Desktop からやり直す。

### `UserResponse.has_password`

パスワードの有無で表示を切り替えるために `GET /v1/auth/me`（`UserResponse`）へ
`has_password: bool` を追加した。ハッシュそのものは返さない。

---

## 8. 他の利用者のプロフィール

テナントのメンバーが、同じテナントに関わる人のプロフィールを見る画面（TASK-257）。
ユーザー名・自己紹介・アバターは本人が §2 のプロフィール設定で編集し、部署・役職・チームは
テナント Admin / オーナーがメンバー設定（`PUT /v1/tenants/{id}/members/{user_id}`）で編集する（TASK-334）。

| 項目 | 値 |
|---|---|
| URL | `/{tenant}/users/{user_id}` |
| ページ | `apps/frontend/src/pages/@tenant/users/@userId/+Page.vue` |
| API | `GET /v1/tenants/{tenant_id}/users/{user_id}`（`UserProfileResponse`） |
| 入口 | タスク詳細の担当者名・コメントの投稿者・テナント設定のメンバー一覧の名前 |

### 返す項目

`id` / `username` / `bio` / `avatar_url` / `department` / `position` / `team` の 7 項目だけ。
部署・役職・チームはこのテナントでの値で、所属行の無いオーナー・客分では null。画面は値のあるものだけを username の下に出す。
メールアドレス・管理者フラグ・凍結フラグ・2FA の有効状態・`has_password` は返さない。
自分自身を指定しても同じ形で返す（画面は自分のときだけ「プロフィールを編集」を出す）。

### 認可

| 状況 | 結果 |
|---|---|
| 閲覧者がテナントのオーナー・テナントメンバーで、対象がそのテナントのオーナー・メンバー・客分 | 200 |
| 閲覧者がテナントに入れない（客分を含む） | 403 |
| 対象がそのテナントに関わっていない / 存在しない | 404 |
| PAT | `read:tenant` を要求する（メンバー一覧と同じ） |

対象がテナントに関わらない場合を 403 にしないのは、`users` がテナント横断の表だからである。
403 と 404 を分けると、任意の UUID について利用者が実在するかを調べられてしまう。
判定は [テナント / プロジェクト認可](../../apps/backend/docs/tenant-project-authz.md) の既存関数
（`is_tenant_owner` / `is_tenant_member` / `explicit_member_project_ids`）だけで行う。
