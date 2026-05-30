# パスワードリセット・変更フロー（運用）

自己サービスのパスワードリセットとログイン中の変更。仕様の正本は [auth-password-reset](/features/auth-password-reset)（Nuxt Content）。

## 運用 UI

| キュー | Board 登録 | 備考 |
|--------|------------|------|
| `verification_email` | **あり**（`http://localhost:3400/`） | 認証メールの監視・再試行 |
| `password_reset_email` | **なし** | ジョブ payload は `user_id` + `email` のみ。トークンはワーカー内で Redis に保存 |

`password_reset_email` を Board に載せない理由: リセット用 bearer secret を運用 UI・バックアップ・トレースに載せないため（CodeRabbit 指摘対応済み）。

## シーケンス図

```mermaid
sequenceDiagram
    autonumber
    actor U as ユーザー
    participant F as フロント
    participant B as API
    participant Q as Apalis (password_reset_email)
    participant R as Redis
    participant S as SMTP

    U->>F: メール入力
    F->>B: POST /v1/auth/password-reset/request
    B->>R: pw_reset:rl:{email}（レート制限）
    alt 登録済み
        B->>Q: enqueue(user_id, email)
        Note over B: 常に 200（enqueue 失敗も外部には成功扱い）
        Q->>R: トークン生成・store_token（ワーカー内）
        Q->>S: リセットメール送信
    end
    B->>F: 200（固定メッセージ）

    U->>F: メールのリンク（token はクエリ）
    F->>B: GET /v1/auth/password-reset/verify?token=...
    B->>R: lookup（消費しない）
    B->>F: 200 / 404

    U->>F: 新パスワード
    F->>B: POST /v1/auth/password-reset/complete
    B->>R: consume_token → user_id
    B->>B: password_hash / sessions_revoked_at / PAT revoke
    B->>F: 200
```

## Redis キー

| キー | 内容 | TTL |
|------|------|-----|
| `pw_reset:t:{token}` | `user_id` | 30 分 |
| `pw_reset:u:{user_id}` | `token` | 30 分 |
| `pw_reset:rl:{email}` | `1`（存在のみ） | 60 秒 |

メールアドレスは `normalize_email` 後の値を `rl:` に使用する。

## 構造化ログ（トークン非記録）

アプリログに **リセットトークン・新パスワード・メール本文 URL の bearer 部分を出力してはならない**。

| `event` フィールド | タイミング | 付与フィールド |
|-------------------|------------|----------------|
| `auth.password_reset.email_queued` | 登録ユーザーへジョブ投入成功 | `user_id` |
| `auth.password_reset.email_sent` | ワーカーが Redis 保存 + SMTP 成功 | `user_id` |
| `auth.password_reset.completed` | `password-reset/complete` 成功 | `user_id` |
| `auth.password_change.completed` | `password/change` 成功 | `user_id` |

### ログに含めてはいけないもの

- `?token=` クエリ（HTTP アクセスログ・リクエストログで URI 全体を出さない）
- `PasswordResetCompleteBody` の `token` / `new_password`
- Apalis ジョブ payload への平文トークン（禁止・実装済み）
- SMTP 送信失敗時の `Debug` でメール本文全体を出すこと（`error = ?e` のみに留める）

リクエストログ middleware は **パスのみ**（`uri.path()`）を記録し、クエリ文字列は含めない。

### 障害時の監視

| ログ / 条件 | 意味 | 対応 |
|-------------|------|------|
| `password reset email enqueue failed`（`warn!`, `user_id` 付き） | DB 上はユーザー存在するがメールキュー投入失敗 | 外部応答は 200 のまま。キュー・DB 接続を確認し手動再送またはユーザーに時間をおいて再試行を案内 |
| `password reset email worker error` | ワーカー異常終了 | Apalis リトライ（最大 8 回）後の failed を確認 |
| `auth.password_reset.completed` が急増 | 大量リセット完了 | 不正利用・クレデンシャルスタッフィング調査 |

## 関連ドキュメント

- 仕様: `docs/content/2.features/auth-password-reset.md`
- メール認証（同型の Apalis パターン）: [email-verification-flow](./email-verification-flow.md)
- 管理者によるリセット: [admin](/features/admin) §7.1（監査ログ `user.password_reset`）
