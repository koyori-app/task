---
title: パスキー認証
description: WebAuthn/FIDO2 によるパスワードなしのログインと、認証手段の追加
status: partial
updated: 2026-11-04
sidebar:
  order: 3
---

## 概要

WebAuthn（FIDO2）を使い、Touch ID・Face ID・セキュリティキーなどをパスキーとして登録・使用できる。実装には [`webauthn-rs`](https://github.com/kanidm/webauthn-rs) を使う。パスキーは利用者の検証（生体または端末の PIN）を必須にして認証するので、「所持（デバイス）+ 生体または PIN」の多要素認証として扱う。パスキーのログインは[2 段階認証](/dev/features/auth/2fa)の TOTP を免除し、パスワードなしの完全なログインとして機能する。既存のメール/パスワードや OAuth に追加で登録でき、代替の認証手段になる。

## 振る舞い

### 登録

登録はログイン済みの利用者が既存のアカウントにパスキーを足す操作である。規則は次のとおりである。

- メールアドレスが確認済みでなければ `403` になる
- 同じ利用者の登録は Redis のロックで 1 つに限り、進行中に開始すると `409`（`passkey-registration-in-progress`）を返す
- ロックとチャレンジの状態の TTL はどちらも 5 分である
- 1 利用者が複数のデバイスに登録できる。上限は 20 個で、登録の開始時に確かめ、完了時にも利用者の行をロックして確かめ直す

```mermaid
sequenceDiagram
    actor U as 利用者
    participant F as ブラウザ
    participant B as API
    participant R as Redis
    participant D as DB

    F->>B: POST /v1/auth/passkeys/registration/start
    B->>R: 登録ロックを取得
    B->>D: 件数の上限と登録済みの credential を確認
    B->>R: 登録の状態を保存（TTL 5 分）
    B-->>F: PublicKeyCredentialCreationOptions
    F->>U: navigator.credentials.create で生体または PIN を要求
    U-->>F: 認証器の応答
    F->>B: POST /v1/auth/passkeys/registration/finish
    B->>R: 登録の状態を取り出して削除
    B->>B: 応答を検証
    B->>D: passkeys に INSERT
    B->>R: 登録ロックを解放
    B-->>F: 201
```

`registration/finish` の失敗は次のとおりである。

| 状況 | 結果 |
|---|---|
| 上限を超える | `409`（`passkey-limit-exceeded`） |
| ロックや状態が無い | `400` |
| 検証に失敗 | `400`（`webauthn-error`）。ロックを解放する |

### ログイン

セッションなしでパスキーだけでログインできる。`start` が発行した `challenge_id` を `finish` に渡して、Redis のチャレンジの状態と対応づける。

```mermaid
sequenceDiagram
    participant F as ブラウザ
    participant B as API
    participant R as Redis
    participant D as DB

    F->>B: POST /v1/auth/passkeys/authentication/start
    B->>R: チャレンジの状態を challenge_id で保存（TTL 5 分）
    B-->>F: PublicKeyCredentialRequestOptions と challenge_id
    F->>F: navigator.credentials.get
    F->>B: POST /v1/auth/passkeys/authentication/finish
    B->>R: チャレンジの状態を取り出して削除
    B->>D: credential_id で passkeys と利用者を取得
    B->>B: 署名と sign_count を検証
    B->>D: sign_count と last_used_at を更新
    B->>B: セッションを発行（2FA なし）
    B-->>F: 204
```

`authentication/finish` の結果は次のとおりである。

| 状況 | 結果 |
|---|---|
| 成功 | `204`。セッションを発行する |
| `challenge_id` の状態が無い | `400` |
| credential が未登録、利用者のメールが未確認、カウンターの逆行 | `401`（`invalid-credentials`） |
| カウンターが非ゼロからゼロに戻った | `401`（`possible-credential-clone`） |
| 凍結済みの利用者 | `403`（`account-suspended`） |
| 署名の検証に失敗 | `400`（`webauthn-error`） |

### Conditional UI

`authentication/start` の `email` を省略すると、discoverable credential 用のチャレンジを返す。`allowCredentials` は空で、`mediation: "conditional"` が付く。フロントエンドはこの指定で `credentials.get()` を呼び、ブラウザがパスワードのオートフィルの UI にパスキーを表示する。

`email` を指定しても、利用者が存在しない・未確認・パスキー未登録なら同じ discoverable のチャレンジに切り替える。パスキーがある場合だけ `allowCredentials` に credential を列挙する。

### 再認証

パスキーの追加と削除は、直近の本人確認を求める。セッション cookie だけで通すと、cookie を盗んだ者が自分の鍵を足して居座れる（足した鍵はパスワードの変更の後も残り、パスキーのログインは TOTP も免除する）。足すのを止めても、正規の鍵を消されて締め出されるので、削除も求める。

| 操作 | 再認証 |
|---|---|
| 登録（`registration/start` と `registration/finish`） | 要る。最初の 1 件目も要る |
| 削除（`DELETE /v1/auth/passkeys/{id}`） | 要る |
| 一覧・名前の変更・パスキーでのログイン | 要らない |

有効期間は 300 秒で、環境変数 `REAUTH_WINDOW_SECS` で変える。満たさなければ `403`（`reauth-required`）を返す。`registration/finish` は、start の後に期間が過ぎたら同じ `403` を返し、登録のロックを放す。

本人確認をした時刻は、セッションの `reauthed_at_ms` に持つ。時刻を立てるのは、資格情報で本人と確かめた次の場合だけである。

| 立てる | 立てない |
|---|---|
| パスワードのログイン（2FA が要る利用者は、2FA の検証で立てる） | OAuth の連携の追加（盗まれたセッションで自分の連携を足して、再認証を満たせてしまう） |
| 2FA の検証（TOTP かリカバリーコード） | TOTP の登録の完了 |
| パスキーのログイン | パスワードの初回設定 |
| OAuth のログイン（連携の追加ではない） | |
| 下の再認証の API | |

手段は次の三つである。パスワードを持たない利用者は、パスキーか連携を使う。メールの確認コードは使わない。

| 手段 | 呼び出し | 失敗 |
|---|---|---|
| パスワード | `POST /v1/auth/reauth/password`（`password`）。成功は `204` | パスワードを持たない利用者は `400`（`password-not-set`）、誤りは `400`（`invalid-current-password`） |
| 既存のパスキー | `POST /v1/auth/reauth/passkey/start`、続けて `POST /v1/auth/reauth/passkey/finish`（`credential`）。自分のパスキーに限る。challenge は Redis に利用者ごとに 1 つ持ち、`GETDEL` で使い捨てにする。成功は `204` | パスキーが無ければ start が `404`（`passkey-not-found`）。他人のパスキーは `401`、start の無い finish は `400` |
| 結んだ OAuth の連携 | `GET /v1/auth/oauth/{provider}?reauth=true`（ログイン済みの利用者のみ）。コールバックで、プロバイダーの利用者が、本人に結んだ連携と一致したら立てる。連携は足さず、セッションも作り直さない | ログインしていなければ開始が `401`。ログインに使えないプロバイダー（`github_app`）は `401`。別の利用者の連携は `403`（`oauth-reauth-mismatch`）で、利用者も連携も増えない |

OAuth の再認証では、プロバイダーにも認証を促すため、`gitlab`・`gitlab_selfhosted`・`oidc` の認可 URL に `prompt=login` を付ける。`github` と `google` には、再認証を強いる手段が無いので付けない。その場合も、連携先の本人でなければ、上の照合で断る。


### 検証の規則

WebAuthn の設定と検証の規則は次のとおりである。

| 項目 | 規則 |
|---|---|
| RP ID | 環境変数 `WEBAUTHN_RP_ID`。省略時は origin のホスト名 |
| origin の検証 | `EMAIL_VERIFICATION_APP_URL` の scheme・host・port を origin とし、`webauthn-rs` が検証する。サブドメインを許す設定はしていない |
| userVerification | `required`。生体認証またはデバイスの PIN を必須にする |
| residentKey | `webauthn-rs` の `start_passkey_registration` の既定で `discouraged`。discoverable な credential を作るかは認証器による |
| sign_count | 応答のカウンターが 0 より大きければ、DB の値より大きいことを必須とする。カウンターが 0 の場合、DB の値も 0 なら「非対応の認証器」として許容し、DB の値が 0 より大きければクローンの疑いとして拒否する |
| challenge の TTL | Redis で 5 分。`GETDEL` で取り出すので使い捨てである |
| crossOrigin と topOrigin | 登録と認証の応答の `clientDataJSON` が `crossOrigin: true` か `topOrigin` を持つか、JSON として読めなければ `400`（`webauthn-error`）。`webauthn-rs` 0.5.5 は登録でしか見ないので、認証の分は task が検める。frame の中から呼ばれた操作を断る。断る時は challenge を取り出す前なので、challenge は消えず（登録ではロックも残る）、同じ challenge に正しい応答を送り直せる |
| credential ID の長さ | 1023 byte を超える credential ID の登録は `400`（`bad-request`）（WebAuthn Level 3 の [Registering a New Credential](https://www.w3.org/TR/webauthn-3/#sctn-registering-a-new-credential) の検証の手順） |
| 最後の認証手段の削除 | パスキーが 1 件で、`password_hash IS NULL` で、ログイン用の `oauth_connections` が 0 件なら `403`（`last-auth-method`）。`github_app` の連携はログインに使えないので数えない |
| メール未確認 | 未確認の利用者は登録できず（`403`）、認証も通らない（`401`） |
| 再認証 | 追加（`registration/start`・`registration/finish`）と削除は、直近 300 秒以内の本人確認を要する。満たさなければ `403`（`reauth-required`）。[再認証](#再認証)を参照 |

環境変数は次のとおりである。

| 変数 | 内容 |
|---|---|
| `WEBAUTHN_RP_ID` | WebAuthn の RP ID。省略時は origin のホスト名 |
| `EMAIL_VERIFICATION_APP_URL` | origin の元になるフロントの URL |
| `REAUTH_WINDOW_SECS` | 再認証の有効期間（秒）。既定は 300 |

## データ

`passkeys` の列は次のとおりである。

| 列 | 型 | 制約 | 説明 |
|---|---|---|---|
| `id` | UUID | PK | |
| `user_id` | UUID | NOT NULL, FK users CASCADE | |
| `credential_id` | BYTEA | NOT NULL, UNIQUE | WebAuthn が生成する認証器固有 ID |
| `public_key` | BYTEA | NOT NULL | `webauthn-rs` の `Passkey` を JSON にしたもの。公開鍵を含む |
| `aaguid` | BYTEA | NULL 可 | 認証器モデルの識別子（16 バイト）。取得できない場合とゼロの場合は NULL |
| `sign_count` | BIGINT | NOT NULL DEFAULT 0 | 署名カウンター。0 はカウンター非対応の認証器 |
| `name` | VARCHAR(255) | NOT NULL | 利用者が付けた名前 |
| `last_used_at` | TIMESTAMPTZ | NULL 可 | |
| `created_at` | TIMESTAMPTZ | NOT NULL DEFAULT now() | |

列の定義は [m20260520000000_initial_schema.rs](https://github.com/koyori-app/task/blob/main/apps/backend/migration/src/m20260520000000_initial_schema.rs) にある。

## API

| メソッド | パス | 説明 |
|---|---|---|
| POST | `/v1/auth/passkeys/registration/start` | 登録チャレンジを発行する |
| POST | `/v1/auth/passkeys/registration/finish` | 登録を完了して DB に保存する（`201`）。`name` は 1〜255 文字 |
| POST | `/v1/auth/passkeys/authentication/start` | 認証チャレンジを発行する。`email` は省略できる |
| POST | `/v1/auth/passkeys/authentication/finish` | 認証を検証してセッションを発行する（`204`） |
| GET | `/v1/auth/passkeys` | 登録済みのパスキーの一覧。`last_used_at` は未使用なら含まない |
| PATCH | `/v1/auth/passkeys/{id}` | 名前を変更する（`204`） |
| DELETE | `/v1/auth/passkeys/{id}` | 削除する（`204`）。[検証の規則](#検証の規則)の最後の認証手段の削除に当たると `403` |
| POST | `/v1/auth/reauth/password` | パスワードで再認証する（`204`）。[再認証](#再認証)を参照 |
| POST | `/v1/auth/reauth/passkey/start` | パスキーでの再認証のチャレンジを発行する。自分のパスキーに限る |
| POST | `/v1/auth/reauth/passkey/finish` | パスキーの署名を検めて再認証する（`204`） |
| GET | `/v1/auth/oauth/{provider}?reauth=true` | 結んだ連携で再認証する（[OAuth](/dev/features/auth/oauth) のコールバックで完了） |

`registration/start` の応答は `webauthn-rs` の `CreationChallengeResponse` をそのまま JSON にしたもので、`publicKey` の下に `challenge`・`rp`・`user`・`pubKeyCredParams` などが入る。`excludeCredentials` に登録済みの credential を渡すので、同じ認証器を二重に登録しにくい。

`authentication/start` の応答は `webauthn-rs` の `RequestChallengeResponse` の JSON に `challenge_id` を足したものである。

```json
{
  "publicKey": {
    "challenge": "base64url-encoded-challenge",
    "rpId": "app.example.com",
    "allowCredentials": [{ "type": "public-key", "id": "base64url-credential-id" }],
    "userVerification": "required"
  },
  "challenge_id": "uuid"
}
```

`registration/finish` の `credential` と `authentication/finish` の `credential` は `PublicKeyCredential` の JSON である。

## 認可

| 操作 | 誰が | スコープ |
|---|---|---|
| 登録・一覧・名前の変更・削除 | ログイン中の本人（`CurrentUser`）。他人のパスキーの ID は `404`（`passkey-not-found`） | セッション専用。有効な Bearer と半認証のセッションは `403` |
| 再認証（パスワード・パスキー） | ログイン中の本人（`CurrentUser`）。他人のパスキーは `401` | セッション専用。半認証のセッションは `403` |
| 認証の開始と完了 | 誰でも | 認証不要 |
| 強制削除（`DELETE /v1/admin/users/{id}/passkeys/{passkey_id}`） | サービス管理者（[サービス管理者の API](/dev/features/admin#api)） | セッション専用 |

## 画面

フロントエンドがパスキーの API を使うのは 1 か所だけである。`/settings/security` の `AuthMethodsSection.vue` が `GET /v1/auth/passkeys` を呼び、最後の認証方法の注意書きを出すために件数を数える（[アカウント設定の画面](/dev/features/account-settings#画面)）。

## 未実装と既知の制限

パスキーの画面は無い。

| 無い画面 | 内容 |
|---|---|
| ログイン画面 | パスキーでのログインボタンと、Conditional UI での候補の表示 |
| セキュリティ設定画面 | `/settings/security` でのパスキーの一覧・追加・名前変更・削除 |
| 再認証の画面 | `reauth-required`（`403`）を受けた時の、パスワード・パスキー・連携での再認証の入口 |

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-11-04 | パスキーを多要素認証として扱い、ログインで TOTP を免除する | 利用者の検証を必須にして認証するので、「所持（デバイス）+ 生体または PIN」が揃う |

| 2026-11-16 | パスキーの追加と削除に、直近 300 秒以内の再認証を求める。手段はパスワード・既存のパスキー・結んだ連携で、メールは使わない。最初の 1 件目の追加にも求める。連携の追加と TOTP の登録は、再認証として数えない | cookie を盗んだ者が自分の鍵を足して居座る、あるいは正規の鍵を消して締め出す攻撃を防ぐ。論文 *The State of Passkeys*（USENIX Security 26）が、登録に確認を求めない site の危うさを挙げる。連携や TOTP の追加を数えると、盗まれたセッションで再認証を満たせてしまう |

### 参考

検証の規則と、再認証・crossOrigin と topOrigin の検め・credential ID の長さの検めは、次の論文の攻撃の型でパスキーの実装を点検した結果を含む。

- 論文: Louis Jannett ほか『The State of Passkeys: Studying the Adoption and Security of Passkeys on the Web』（35th USENIX Security Symposium、2026）。[発表ページ](https://www.usenix.org/conference/usenixsecurity26/presentation/jannett)。103 サイトを評価し、18 サイトで重大、53 サイトで高い重大度の攻撃が通ったと述べる。
- きっかけ: ITmedia NEWS の記事『パスワードより安全なはずが……パスキー対応103サイトを検証→全てに脆弱性 独大学の調査結果』（2026-11-16）。[記事](https://www.itmedia.co.jp/news/article/2611/16/2000003199/)。

直しと、論文の型の対応は次のとおりである。

| 直し | 論文の型と章 |
|---|---|
| credential ID の長さの検め（TASK-546） | 型は「Length of credid」。表「Evaluation Results and Test Catalog」に載る |
| crossOrigin と topOrigin の検め（TASK-545） | 型は「Framing」。章「CWE-1021: Improper Restriction of Rendered UI Layers or Frames」 |
| パスキーの追加と削除の再認証（TASK-543） | 型ではなく所見。章「Part 2: How Websites Implement Passkeys」の「Confirming Passkey Registrations」と「Removing Passkeys」 |
| ログイン開始でのメールごとの有無の扱い（TASK-544。今は直さず、課題として残す） | 型は「Allow Credentials」。章「CWE-204: Observable Response Discrepancy」 |
