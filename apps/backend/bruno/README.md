# Bruno: 登録〜メール確認

バックエンド (`http://localhost:3400`) の認証フローを [Bruno](https://www.usebruno.com/) で試すコレクションです。

## 開き方

1. Bruno で **Open Collection** を選ぶ
2. このフォルダ `apps/backend/bruno` を指定する
3. 右上の環境で **local** を選択する

## 実行順（推奨）

| 順 | リクエスト | 期待 |
|----|-----------|------|
| 1 | `1. 新規登録` | 201 |
| — | `verificationToken` を Env に設定（下記） | |
| 2 | `2. メールアドレス確認` | 200 |
| 3 | `4. ログイン` | 204（Cookie 保存） |
| 4 | `5. 自分の情報` | 200、`email_verified: true` |

### `verificationToken` の取り方

1. 認証メールの `http://localhost:3000/verify-email?token=XXXX` の `XXXX`
2. Apalis Board `http://localhost:3400/` のジョブ引数
3. `（参考）認証メールジョブ一覧` を実行（取れた場合は post-response で Env に自動設定）

取れないときは `（任意）認証メール再送` → 上記を繰り返す（60 秒クールダウン）。

## 環境変数 (`environments/local.bru`)

| 変数 | 例 |
|------|-----|
| `baseUrl` | `http://localhost:3400` |
| `email` | 未登録のメールアドレス |
| `password` | 8 文字以上 |
| `username` | 3 文字以上 |
| `verificationToken` | メール等からコピー |

## 注意

- ログインは **Cookie セッション** — Bruno で Cookie を有効にし、同じコレクションで続けて実行する
- 同じ `email` で再テストする場合は未使用アドレスに変えるか DB をリセットする
