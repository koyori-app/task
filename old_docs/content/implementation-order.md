---
title: 実装順序・並行実行判断表
description: 全仕様書の依存関係と並行実装可否を一覧で管理する
icon: lucide:git-branch
---

# 実装順序・並行実行判断表

> 更新日: 2026-05-27 — 仕様書を追加・変更したら必ずこのファイルも更新すること。

---

## 並行実行ルール

- **同じウェーブ内のPRはすべて並行開発・レビュー可能**
- **前のウェーブが全部マージ済みになってから次のウェーブを始める**
- 並行開発できても同一テーブルを ALTER するマイグレーションはマージ時に順番を揃えること（開発は並行、デプロイは逐次）
- フロントエンド（Phase B）はバックエンド全ウェーブ完了後に着手

---

## ウェーブ別実装グループ

### Wave 0 — 依存なし（すべて並行可）

| 仕様書 | PR | 共有テーブル変更 |
|--------|-----|----------------|
| [auth-oauth](/features/auth-oauth) | — | `users.password_hash` NOT NULL 解除、`oauth_connections` 新規 |
| [auth-2fa](/features/auth-2fa) | — | `users.totp_enabled`, `tenants.require_2fa` 追加 |
| [auth-passkeys](/features/auth-passkeys) | — | `passkeys` 新規 |
| [auth-password-reset](/features/auth-password-reset) | — | `users.sessions_revoked_at` 追加 |
| [admin](/features/admin) | — | `users.is_admin`, `users.is_suspended` 追加、`audit_logs` 新規 |
| [drive](/features/drive) | feat/drive | `tenants.drive_quota_bytes` 追加、`drive_folders`・`drive_files`・`drive_folder_shares` 新規 |
| [tasks #1 コア](/features/tasks/core) | #1 | `tasks` 他多数（独立した新規テーブル群） |
| [tasks #9a GitHub App 基盤](/features/tasks/github-app) | #9a | `github_integrations` 新規（`projects`・`users` のみ参照） |

> **注意 — auth系の `users` テーブル競合**: auth-oauth・auth-2fa・auth-passkeys・auth-password-reset・admin はすべて `users` テーブルを変更する。マイグレーションをマージする際は順番を決めて適用すること（どの順でも機能上問題ない）。

### Wave 1 — tasks #1（コア）完了後（Wave 1内は並行可）

| 仕様書 | PR | 追加の依存 |
|--------|-----|----------|
| [tasks #2 作業時間追跡](/features/tasks/time-tracking) | #2 | tasks #1 のみ |
| [tasks #3 スプリント](/features/tasks/sprints) | #3 | tasks #1 のみ |
| [tasks #4 コメント・アクティビティ](/features/tasks/collaboration) | #4 | tasks #1 のみ |
| [tasks #6 カスタムフィールド](/features/tasks/custom-fields) | #6 | tasks #1 のみ |
| [tasks #7 検索・バルク・ビュー・添付](/features/tasks/extensions) | #7 | tasks #1 + drive（どちらも Wave 0） |
| [tasks #10 Webhook](/features/tasks/webhooks) | #10 | tasks #1 のみ |

> `#7`（添付機能）は drive も完了している必要がある。tasks #1 と drive は同じ Wave 0 なので Wave 1 開始時点で両方完了している前提。

### Wave 2 — tasks #4（コメント）完了後

| 仕様書 | PR | 追加の依存 |
|--------|-----|----------|
| [tasks #5 通知・ウォッチャー](/features/tasks/notifications) | #5 | tasks #1 + #4 |

### Wave 3 — tasks #5（通知）完了後

| 仕様書 | PR | 追加の依存 |
|--------|-----|----------|
| [tasks #8 自動化](/features/tasks/automation) | #8 | tasks #1 + #4 + #5 |

### Wave 4 — tasks #8 + #9a 完了後

| 仕様書 | PR | 追加の依存 |
|--------|-----|----------|
| [tasks #9b GitHub↔タスク連携](/features/tasks/github-tasks) | #9b | tasks #1 + #8 + #9a |

### Wave 5 — 全バックエンド完了後

| 仕様書 | 対応範囲 |
|--------|---------|
| 各仕様書末尾「Phase B」節 | フロントエンド実装（全機能一括） |

---

## クイックリファレンス：A と B は並行できる？

| 組み合わせ | 並行可否 | 理由 |
|-----------|---------|------|
| auth-oauth ↔ auth-2fa | ✅ 可 | 両方 Wave 0、独立 |
| auth-* ↔ tasks #1 | ✅ 可 | 両方 Wave 0、テーブル非重複 |
| auth-* ↔ drive | ✅ 可 | 両方 Wave 0、独立 |
| tasks #1 ↔ tasks #9a | ✅ 可 | 両方 Wave 0 |
| tasks #2 ↔ tasks #3 | ✅ 可 | 両方 Wave 1 |
| tasks #2 ↔ tasks #4 | ✅ 可 | 両方 Wave 1 |
| tasks #4 ↔ tasks #5 | ❌ 不可 | #5 は #4 が必要（Wave 1 → Wave 2） |
| tasks #5 ↔ tasks #8 | ❌ 不可 | #8 は #5 が必要（Wave 2 → Wave 3） |
| tasks #8 ↔ tasks #9b | ❌ 不可 | #9b は #8 が必要（Wave 3 → Wave 4） |
| tasks #9a ↔ tasks #9b | ❌ 不可 | #9b は #9a が必要（Wave 0 → Wave 4） |
| admin ↔ auth-passkeys | ✅ 可 | 両方 Wave 0（admin は passkeys API を呼ぶが、同一 PR 内では不要） |

---

## 依存関係グラフ（全体）

```text
Wave 0（並行）
├── auth-oauth
├── auth-2fa
├── auth-passkeys
├── auth-password-reset
├── admin
├── drive ──────────────────────────────────────┐
├── tasks #1（コア）                              │
│       │                                        │
└── tasks #9a（GitHub App）                      │
        │                                        │
        │  Wave 1（#1 完了後・並行）              │
        │  ├── tasks #2（作業時間追跡）            │
        │  ├── tasks #3（スプリント）              │
        │  ├── tasks #4（コメント）               │
        │  ├── tasks #6（カスタムフィールド）       │
        │  ├── tasks #7（検索・添付） ←───────────┘
        │  └── tasks #10（Webhook）
        │           │
        │       Wave 2（#4 完了後）
        │       └── tasks #5（通知）
        │               │
        │           Wave 3（#5 完了後）
        │           └── tasks #8（自動化）
        │                       │
        │                   Wave 4（#8 + #9a 完了後）
        └───────────────────────└── tasks #9b（GitHub↔タスク）
```

---

## 仕様書追加・更新時のメンテナンス手順

新しい仕様書を追加・更新したとき（`shogun-spec-writer` スキルが自動実行）:

1. 仕様書の `> 依存:` 行を確認して依存先を特定
2. 依存先のウェーブ番号 + 1 が自分のウェーブ番号
   - 依存なし → Wave 0
   - 最大依存先のウェーブが N → 自分は Wave N+1
3. 上の「ウェーブ別実装グループ」表に追記
4. 「クイックリファレンス」に代表的な組み合わせを追記
5. 「依存関係グラフ」の ASCII 図を更新
