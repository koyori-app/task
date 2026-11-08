---
title: 設計の記録
description: 認可・認証・検索・Redis について、判断の理由と比較した案を残すページ
updated: 2026-11-05
sidebar:
  group:
    label: 設計の記録
  order: 30
---

判断の理由と比較した案を残すページをまとめた群である。現在の API の形と振る舞いの定義元は機能の仕様である。

- 認可は[テナントとプロジェクトの認可](/dev/design/tenant-project-authz)と[個人アクセストークンの認可](/dev/design/personal-access-tokens-authz)に書く
- 認証メールとワーカーは[メール認証のフロー](/dev/design/email-verification-flow)、リセットは[パスワードリセットのフロー](/dev/design/password-reset-flow)に書く
- 検索は[タスクの類似検索](/dev/design/task-similarity-search)、Redis の扱いは [Redis の信頼境界](/dev/design/redis-trust-boundary)に書く
- タスクの変更を画面へ伝える仕組みは[プロジェクトのイベント](/dev/design/project-events)に書く
