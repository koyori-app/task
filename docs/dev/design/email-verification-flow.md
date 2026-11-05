---
title: メール認証のフロー
description: 登録からメール確認・ログインまでのシーケンスと、Apalis ワーカーの監督と復帰
updated: 2026-11-04
sidebar:
  order: 4
---

登録からメール確認・ログインまでのシーケンスを記述する（Apalis + PostgreSQL ジョブキュー）。

運用 UI として [Apalis Board](https://github.com/apalis-dev/apalis-board) を `http://localhost:3400/` で提供する（キュー `verification_email` の監視・再試行状況）。ジョブペイロードにメールアドレスが含まれるため、board の UI/API は管理者セッション必須である。認証トークンはペイロードに載せず、ワーカーが生成して Redis のみに保持する。

## シーケンス図

流れは次のとおりである。

```mermaid
sequenceDiagram
    autonumber
    actor U as 利用者
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
    B->>DB: トランザクション<br/>利用者作成（未認証）
    B->>Q: Apalis にジョブ投入<br/>(user_id, email)
    alt エンキュー成功
        B->>F: 201 Created
    else エンキュー失敗
        Note over B,F: 利用者は未認証のまま残す
        B->>F: 503（認証メール再送の案内）
    end
    F->>U: メール確認の案内を表示
    par バックグラウンド送信（Apalis）
        A->>Q: ジョブ取得（NOTIFY + ポーリング）
        A->>R: トークンと issued_at を生成して保存（世代比較・Lua で原子的）
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
    B->>DB: 利用者の状態を確認
    alt 送信可能な未認証の利用者
        B->>Q: Apalis にジョブ投入
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

登録済みのアドレスで登録を受けたときは、メールアドレスの列挙を防ぐため同じ 201 を返し、認証メールの代わりに `already_registered_email` のジョブで通知メールを送る。

## 旧 Outbox 方式からの変更点

変更点は次のとおりである。

| 項目 | 以前 | 現在 |
|---|---|---|
| キュー | `verification_email_outbox` テーブル + ポーリング | Apalis `PostgresStorage`（`verification_email` キュー） |
| 登録後 | 同一 TX で outbox 行 + `wake_worker` | TX は利用者の INSERT のみで、コミット後に `enqueue`（失敗時は 503・再送で回復） |
| ワーカー | 自前 supervisor・SKIP LOCKED | Apalis ワーカー（NOTIFY・リトライ・グレースフルシャットダウン） |
| 失敗処理 | `attempts` / `failed` 列を手動更新 | Apalis `RetryPolicy`（最大 8 回） |
| 運用 UI | なし | Apalis Board（`/`） |
| ワーカーの生存回復 | 自前 supervisor | `job::supervise` が終了した世代を別 ID で作り直す（[ワーカーの監督と復帰](#ワーカーの監督と復帰)） |

## ワーカーの監督と復帰

全キュー（7 本）の apalis ワーカーは `job::supervise` の監督ループの下で動く。apalis のワーカーは heartbeat が失敗すると終了し、プロセスも HTTP も生きたままジョブが Pending で滞留する。本番では DB の IO 停滞でプールの取得が 30 秒を超え、7 本すべてが終了して 19 時間滞留した。

### 世代別 ID

`apalis.workers.id` は `{queue}-worker-{プロセス起動時の UUID}-{世代番号}` である。同じ ID で登録し直すと 2 つの問題が起きる。

- `register.sql` の更新はセッション単位の advisory lock 付きなので、別のプール接続からだと `WORKER_ALREADY_EXISTS` で即死する
- `last_seen` が更新され続けるので、死んだ世代が掴んでいた Running / Queued のジョブが孤児回収（`last_seen` から 5 分）の対象にならない

ID を変えれば旧行の `last_seen` が止まり、5 分後に新世代の heartbeat がそのジョブを Pending に戻す。旧行は `jobs.lock_by` から参照されるので消さない。

### 監督ループ

shutdown の前に世代が終わったら、Ok・Err・panic を問わず待ってから作り直す。待ちは 1 秒から倍々で最大 60 秒である。5 分以上動いた世代の後は 1 秒に戻す。shutdown の後は結果を問わず作り直さない。

世代は `run_until` ではなく ctx の `stop()` で止める。`run_until` は起動前に signal が来ると停止が捨てられ、ワーカーが止まらなくなる。

### 保証

再試行はされるが exactly-once ではない。ACK の前に世代が終わったジョブは孤児回収の後に再実行されるので、メールが重複して届くことがある。メール以外のジョブ（GitHub 連携・レビュー要約など）も同じである。

### プール

`job::setup_pool` は 16 本である。ワーカーごとに LISTEN 用の接続が 1 本ずつ常時占有される（7 本）ので、残りの 9 本を heartbeat・poll・孤児回収・ハンドラーからのジョブ投入が共有する。これは通常時の予算で、世代交代中は旧世代のリスナーの接続が UNLISTEN まで一時的に残る。

### Board の表示

ワーカー一覧には古い世代の行も残る。ctx を監督側で作るので、`layers` 列はサービスの型名ではなく `()` になる。

## 未実装と既知の制限

- heartbeat の更新が 0 件になる事象（`WORKER_DOES_NOT_EXIST`）が本番で 1 回観測されたが、原因は分かっていない。起きても監督が作り直すので復帰はする

## 決定の記録

| 日付 | 決定 | 理由 |
|---|---|---|
| 2026-05-24 | 自前の Outbox をやめて Apalis のキューに移す | 記録なし |
| 2026-10-24 | 全キューのワーカーを `job::supervise` の監督ループの下で動かし、世代別 ID で作り直す | DB の IO 停滞で 7 本すべてが終了し、ジョブが 19 時間滞留した |
