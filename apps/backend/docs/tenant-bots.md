# テナント BOT の認証と管理

## データモデル

`users.account_kind` は `human` または `bot` とし、既存の利用者は既定値 `human` になる。
BOT は `users.bot_tenant_id` に所有テナントを一つ持ち、通常の `tenant_members` を必ず作成する。
既存の `project_members`、役割、`personal_tokens` を共有し、認可を二重に実装しない。

BOT のメールは UUID から作る `@bots.task.invalid` のアドレスとする。
メールの参照が本人向け DTO、OAuth の照合、招待、本文テンプレートに広がっているため、今回の変更では nullable にしない。
人の新規登録ではこのドメインを拒否し、マイグレーションの制約で BOT のアドレスをその UUID に結び付ける。
既存の利用者はアドレスにかかわらず `human` のまま移行する。
同じ制約で BOT のパスワード、TOTP の有効化、プラットフォーム管理者への昇格を拒む。

## メール用途の棚卸し

| 用途 | 実装 | BOT の扱い |
| --- | --- | --- |
| 通知、割り当て、コメント、メンション、レビュー | `service::notifications` → `job::notification_email::send_one` | 利用者種別で送信待ちを破棄し、SMTP の直前でも専用アドレスを除外 |
| 新規登録と認証メールの再送 | `handlers::auth::{register,resend_verification_email}` → `job::verification_email` | 専用ドメインの登録を拒否し、BOT には再送しない。最後に SMTP でも除外 |
| メールの本人確認 | `handlers::auth::verify_email` | BOT のトークンを受け付けず、認証済みに変更しない |
| 登録済みアドレスへの案内 | `job::already_registered_email` → `service::already_registered_email_delivery` | SMTP の直前で除外 |
| パスワードの再設定 | `handlers::password_reset` → `job::password_reset_email` | BOT は要求時に送信を予約せず、トークン検証と完了も拒否 |
| システム管理者の再設定と宛先の上書き | `handlers::admin_users::password_reset` → `service::password_reset_delivery` | BOT はメールの予約前に拒否。宛先を人の実アドレスへ変えた場合も拒否 |
| 再設定の本文生成の二系統 | `service::{password_reset_delivery,password_reset_email_delivery}` | 両方とも共通の SMTP 判定で除外 |
| テナントへの招待と再送 | `handlers::tenant_invitations` → `job::tenant_invitation_email` → `service::tenant_invitations::send_invitation_email` | BOT のアドレスへの招待を拒否し、古いジョブも SMTP の直前で除外 |
| OAuth 初回登録時の確認 | `handlers::oauth::create_oauth_user_and_connection` → `job::verification_email` | BOT 専用ドメインは人として保存できず、SMTP でも除外 |
| デスクトップ通知 | 端末トークンと通知 API | メール送信ではない。BOT の端末認証は拒否 |

`SmtpClient::send_email` の除外は実 SMTP と試験用 Capture の分岐より前に置く。
各テンプレートから BOT と人の対照アドレスへ送る統合試験で、BOT の記録が Capture に一件も入らないことを確かめる。

## 対話用の入口

以下のパスには `/v1` を付ける。

| ハンドラー | ルート | 拒否する位置 |
| --- | --- | --- |
| `auth::login` | `POST /auth/login` | BOT 専用アドレスと共通セッション発行処理 |
| `auth::register` | `POST /auth/register` | BOT 専用ドメインを人の登録に使う操作 |
| `auth::{verify_email,resend_verification_email}` | `POST /auth/{verify-email,resend-verification-email}` | 利用者種別 |
| `passkeys::{registration_start,registration_finish}` | `POST /auth/passkeys/registration/{start,finish}` | セッションの利用者種別 |
| `passkeys::{authentication_start,authentication_finish}` | `POST /auth/passkeys/authentication/{start,finish}` | 開始はアカウント有無を隠す既存挙動を保ち、署名確認後のセッション発行前に BOT を拒否 |
| `oauth::oauth_start` | `GET /auth/oauth/{provider}` | 認証済み利用者の種別を確認し BOT を拒否する |
| `oauth::oauth_callback` | `GET /auth/oauth/{provider}/callback` | 既存接続の利用者と接続追加先、共通セッション発行処理 |
| `oauth::set_initial_password` | `POST /auth/password` | セッションの利用者種別 |
| `auth_2fa::{totp_setup,totp_verify_setup}` | `POST /auth/2fa/totp/{setup,verify-setup}` | 半認証を含むセッションの利用者種別 |
| `auth_2fa::verify_2fa` | `POST /auth/2fa/verify` | 半認証セッションの利用者種別 |
| `password_reset::{password_reset_request,password_reset_verify,password_reset_complete}` | `POST /auth/password-reset/{request,verify,complete}` | 要求は存在を隠す既存の 200 応答で配信を抑止し、トークン確認と更新では拒否 |
| `password_reset::password_change` | `POST /auth/password/change` | セッションの利用者種別 |
| `desktop_auth::create_desktop_auth_code` | `POST /desktop/auth/codes` | 人のセッションを必須にする |
| `desktop_auth::exchange_desktop_auth_token` | `POST /desktop/auth/token` | コードを引いた後の利用者種別 |
| `tenant_invitations::accept_invitation` | `POST /invitations/accept` | 人のセッションを必須にする |
| `auth::{me,update_me}`、管理用の extractor | `GET/PATCH /auth/me` ほか | 保存済みセッションから利用者を読むたびに BOT を拒否 |

PAT の照合では `is_suspended` と所有テナントを確認する。
BOT に対する Device Token とブラウザーのセッションは、保存済みの値が残っていても利用者の取得時に拒否する。

## テナントの 2FA 強制

BOT はテナントの 2FA の強制（`require_2fa`）の対象から外す。
BOT は TOTP もパスキーも持てず、セッションにも入れないので、強制の下では PAT で業務の口を使えなくなるため。
2FA は人の対話のサインインを守る物で、BOT は PAT でしか入らない。
BOT の作成と PAT の発行は 2FA の強制を満たした人の Admin だけができるので、強制の意図は発行の所で保つ。
免除は BOT に限り、2FA を満たさない人の PAT は従来どおり 403 になる。

## 管理 API

| メソッド | パス | 用途 |
| --- | --- | --- |
| GET / POST | `/v1/tenants/{tenant_id}/bots` | 一覧と作成 |
| PATCH | `/v1/tenants/{tenant_id}/bots/{bot_id}` | 停止と再開 |
| GET / POST | `/v1/tenants/{tenant_id}/bots/{bot_id}/tokens` | PAT のメタデータ一覧と発行 |
| DELETE | `/v1/tenants/{tenant_id}/bots/{bot_id}/tokens/{token_id}` | PAT の取り消し |

管理 API は人のセッションに限定し、`tenant_members` と同じく `ensure_tenant_access` を通す。
テナントが 2FA を強制していれば、2FA を満たさない Admin は全ての管理 API で 403（`2fa-setup-required`）になる。
既存の Admin ロールにはメンバーを追加し昇格させる権限があるため、Admin の BOT に `write:tenant` を与えると、人への権限委任もできる。
BOT の直接の PAT 発行拒否と、既存の Admin による委任の範囲は区別して扱う。
書き込みと同じトランザクションで管理者の利用者行、テナント行、所属行を共有ロックし、権限確認の直後に降格される競合を防ぐ。
PAT の発行と停止は同じ BOT 行を排他ロックする。
発行時は必須の期限を現在より後、365 日以内に制限する。
画面は端末の時計から期限を作るので、上限には時計の食い違いに備えて 5 分の猶予を足す。
プロジェクト指定は所有テナント内だけに許す。

作成、PAT の発行と取り消し、停止と再開を `audit_logs` に同じトランザクションで記録する。
既存のテナント全体の PAT 一括失効でも、対象の BOT ごとに鍵の取り消しを記録する。
操作者とテナント、BOT の ID、操作種別、必要な場合は PAT の ID を記録し、平文の鍵やハッシュは載せない。
`GET /v1/personal_tokens/me` は既存の応答に `account_kind` を追加する。
既存 CLI は追加フィールドを無視できるため、コマンドや認証方式を変更しない。

## 利用者の表示

| 表示箇所 | データと部品 |
| --- | --- |
| タスクの担当者と作成者 | `UserSummary.account_kind` と `UserAvatar` |
| 担当者の選択候補 | `UserSummary.account_kind` と `UserSelector` の補足の字（BOT アカウント） |
| コメントと返信の投稿者 | `CommentUser.account_kind` と `UserAvatar` |
| タスクの操作履歴 | `ActivityUser.account_kind` と `TaskActivityEntry`、`UserAvatar` |
| ダッシュボードの最近の動き | `ActivityUser.account_kind` と `UserAvatar` |
| テナントとプロジェクトのメンバー | `UserSummary.account_kind` と `UserAvatar` |
| カレンダーの担当者 | `UserSummary.account_kind` と `CalendarTimeline` の行データ、`UserAvatar` |
| BOT 管理の一覧 | `BotResponse.account_kind` と `TenantBotsSection` |
| 利用者のプロフィール | `UserProfileResponse.account_kind` と `UserAvatar` |
| 自分のプロフィールとアカウントメニュー | BOT はセッションへ入れないため、人の表示だけを扱う |
| メンション本文 | 既存の名前解決と所属判定を共有する。本文の構文は変更しない |

## 停止とロールバック

BOT 単体の削除 API は設けない。
汎用の利用者管理 API では BOT の権限変更、削除、認証情報の操作を拒否し、テナントの BOT 管理へ集約する。
停止して利用者レコードを残すことで、コメントなどの投稿者を維持する。
テナント全体を削除する場合は、既存の削除処理に従ってそのテナントの BOT も削除する。
停止時に既存 PAT を取り消すため、再開後も古い鍵は使えない。

マイグレーションの down は、BOT の PAT を取り消し、利用者を停止し、セッション失効時刻を進めてから追加列と制約を取り除く。
旧版へ戻した後も、BOT だった利用者の履歴は停止中の利用者として残る。
再び up した場合、全レコードの種別は `human` になるため、BOT の所有情報を復元するにはロールバック前のバックアップが必要になる。
