# メール認証フロー

登録からメール確認・ログインまでのシーケンス（Apalis + PostgreSQL ジョブキュー）。

運用 UI: [Apalis Board](https://github.com/apalis-dev/apalis-board) を `http://localhost:3400/` で提供（キュー `verification_email` の監視・再試行状況）。

## シーケンス図

```mermaid
sequenceDiagram
    autonumber
    actor U as ユーザー
    participant F as フロント(Web)
    participant B as API(バックエンド)
    participant A as Apalisワーカー<br/>(同一プロセス)
    participant Q as PostgreSQL<br/>(Apalis ジョブキュー)
    participant DB as PostgreSQL<br/>(アプリ DB)
    participant R as Redis
    participant S as SMTPサーバ
    participant M as メール

    rect rgb(240, 248, 255)
    Note over U,F: アカウント作成
    U->>F: 登録情報を入力
    F->>B: POST /v1/auth/register
    B->>DB: トランザクション<br/>ユーザー作成（未認証）
    B->>Q: Apalis にジョブ投入<br/>(user_id, email, token, issued_at)
    alt エンキュー成功
        B->>F: 201 Created
    else エンキュー失敗
        Note over B,F: ユーザーは未認証のまま残す
        B->>F: 503（認証メール再送の案内）
    end
    F->>U: メール確認の案内を表示
    par バックグラウンド送信（Apalis）
        A->>Q: ジョブ取得（NOTIFY + ポーリング）
        A->>R: 認証トークン保存（issued_at 世代・Lua で原子的）
        A->>S: 認証メール送信
        S->>M: 配信
        A->>Q: ジョブ完了（成功）
    and 送信失敗時
        A->>Q: リトライ（最大 8 回・Apalis RetryPolicy）
        Note over A,Q: 失敗が続くとジョブは failed 相当で打ち切り<br/>古い issued_at のリトライは Redis 更新・送信をスキップ
    end
    end

    rect rgb(245, 255, 245)
    Note over U,F: メールのリンクからフロントへ
    U->>M: メール受信・リンクをクリック
    M-->>U: ブラウザでフロントを開く<br/>（token は URL エンコード済み）
    U->>F: /verify-email?token=... でアクセス
    F->>F: URL から token を取得（デコード）
    end

    rect rgb(255, 250, 240)
    Note over F,B: 認証完了（副作用は API が担当）
    F->>B: POST /v1/auth/verify-email（JSON: token）
    B->>R: トークン検証・消費（GETDEL + Lua）
    alt トークン有効
        B->>DB: email_verified = true に更新
        B->>F: 200 OK
        F->>U: 認証完了の表示
    else 無効・期限切れ
        B->>F: 400
        F->>U: エラー／再送案内
    end
    end

    rect rgb(255, 245, 245)
    Note over U,F: 認証メールの再送（任意）
    U->>F: 再送を依頼
    F->>B: POST /v1/auth/resend-verification-email
    B->>R: 再送間隔チェック（SET NX・60秒）
    B->>DB: ユーザー状態を確認
    alt 送信可能な未認証ユーザー
        B->>Q: Apalis にジョブ投入（新トークン）
        B->>F: 200 OK
        F->>U: 「送信しました」等
        A->>Q: ジョブ取得
        A->>R: 新トークン保存（より新しい issued_at のみ・旧トークン無効化）
        A->>S: メール再送
        S->>M: 配信
        A->>Q: ジョブ完了
    else 該当なし／既認証など
        B->>F: 404 / 429 / 409
        F->>U: 案内のみ（詳細は API 設計どおり）
    end
    end

    rect rgb(248, 248, 255)
    Note over U,F: ログイン（認証後）
    U->>F: メール・PWでログイン
    F->>B: POST /v1/auth/login
    alt 未メール確認
        B->>F: 403（message: email-not-verified）
        F->>U: メール確認の案内
    else メール確認済み
        B->>F: 204 No Content（セッション Cookie）
        F->>U: ログイン後画面へ
    end
    end
```

## 旧 Outbox 方式からの変更点

| 項目 | 以前 | 現在 |
|------|------|------|
| キュー | `verification_email_outbox` テーブル + ポーリング | Apalis `PostgresStorage`（`verification_email` キュー） |
| 登録後 | 同一 TX で outbox 行 + `wake_worker` | TX はユーザー INSERT のみ → コミット後に `enqueue`（失敗時は 503・再送で回復） |
| ワーカー | 自前 supervisor・SKIP LOCKED | Apalis ワーカー（NOTIFY・リトライ・グレースフルシャットダウン） |
| 失敗処理 | `attempts` / `failed` 列を手動更新 | Apalis `RetryPolicy`（最大 8 回） |
| 運用 UI | なし | Apalis Board（`/`） |
