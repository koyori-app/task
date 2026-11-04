---
title: Koyori Docs
description: Koyori の仕様書と開発ドキュメント
updated: 2026-11-04
sidebar:
  order: 0
---

Koyori の機能仕様、フロントエンドの仕様、開発手順をまとめたサイトである。読みたい内容の群から入る。バックエンドの設計文書は [apps/backend/docs](https://github.com/koyori-app/task/blob/main/apps/backend/docs/README.md) にある。

## 機能仕様

各機能の振る舞い、データ、API、認可、画面を機能ごとに書く。

### アカウントと管理

ログインの方法、アカウント設定、サービス全体の管理を扱う。

- [アカウント設定](/features/account-settings)
- [サービス管理者と監査ログ](/features/admin)
- [2 段階認証](/features/auth-2fa)
- [OAuth 認証](/features/auth-oauth)
- [パスキー認証](/features/auth-passkeys)
- [パスワードリセットと変更](/features/auth-password-reset)

### タスク

タスクとその周辺の機能を扱う。全体の案内は[タスク機能の概要](/features/tasks)にある。

- [タスクのコア](/features/tasks/core)
- [作業時間追跡](/features/tasks/time-tracking)
- [スプリント](/features/tasks/sprints)
- [コメントとアクティビティ](/features/tasks/collaboration)
- [メンションのクリップボード形式](/features/tasks/collaboration-clipboard)
- [通知とウォッチャー](/features/tasks/notifications)
- [カスタムフィールド](/features/tasks/custom-fields)
- [検索・バルク・ビュー・添付](/features/tasks/extensions)
- [自動化](/features/tasks/automation)
- [GitHub App 基盤](/features/tasks/github-app)
- [Git ホスティングとタスクの連携](/features/tasks/github-tasks)
- [Webhook](/features/tasks/webhooks)
- [スケジュール](/features/tasks/schedule)
- [山積み・手動の山崩し](/features/tasks/workload)

### その他の機能

- [Drive](/features/drive)
- [レビュー指摘](/features/review-findings)

## フロントエンド

URL、共通レイアウト、フォーム、本文の描画、PWA など、画面をまたぐ実装を書く。

- [URL](/frontend/url-spec)
- [パンくず](/breadcrumbs)
- [認証フォーム](/frontend/auth-forms)
- [KFM の描画](/frontend/kfm-phase1-implementation)
- [PWA](/frontend/pwa)

## 開発手順

スキーマの変更と依存の管理の手順を書く。

- [マイグレーション](/development/database-migrations)
- [pnpm の lockfile](/development/pnpm-lockfile)
