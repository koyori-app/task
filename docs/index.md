---
title: Koyori Docs
description: Koyori の仕様書と開発ドキュメント
sidebar:
  order: 0
---

Koyori の機能仕様、フロントエンドの仕様、開発手順をまとめたサイトである。
バックエンドの設計文書は [apps/backend/docs](https://github.com/koyori-app/task/blob/main/apps/backend/docs/README.md) にある。

## 機能仕様

アカウントと管理:

- [アカウント設定・プロフィール編集](/features/account-settings)
- [サービス管理者・監査ログ](/features/admin)
- [2段階認証（2FA）](/features/auth-2fa)
- [OAuth ログイン × 2FA 統合（フォローアップ）](/features/auth-2fa-oauth-followup)
- [OAuth 認証拡張](/features/auth-oauth)
- [パスキー認証](/features/auth-passkeys)
- [パスワードリセット・変更](/features/auth-password-reset)

タスク:

- [タスク機能 概要](/features/tasks)
- [タスク コア](/features/tasks/core)
- [作業時間追跡](/features/tasks/time-tracking)
- [スプリント](/features/tasks/sprints)
- [コメント・アクティビティ](/features/tasks/collaboration)
- [コメントのメンションのクリップボード形式](/features/tasks/collaboration-clipboard)
- [通知・ウォッチャー](/features/tasks/notifications)
- [カスタムフィールド](/features/tasks/custom-fields)
- [検索・バルク・ビュー・添付](/features/tasks/extensions)
- [自動化](/features/tasks/automation)
- [GitHub App 基盤](/features/tasks/github-app)
- [Git ホスティング↔タスク連携](/features/tasks/github-tasks)
- [Webhook](/features/tasks/webhooks)
- [スケジュール](/features/tasks/schedule)
- [山積み・手動の山崩し](/features/tasks/workload)

その他:

- [Drive 機能仕様](/features/drive)
- [レビュー指摘管理](/features/review-findings)

## フロントエンド

- [フロントエンド URL 仕様書](/frontend/url-spec)
- [共通レイアウトのパンくず](/breadcrumbs)
- [認証フォーム 設計書](/frontend/auth-forms)
- [KFM Phase 1 実装ノート（github profile）](/frontend/kfm-phase1-implementation)
- [PWA（アプリとしての install）仕様書](/frontend/pwa)

## 開発手順

- [マイグレーション / スキーマファースト運用](/development/database-migrations)
- [pnpm の lockfile 構成](/development/pnpm-lockfile)
