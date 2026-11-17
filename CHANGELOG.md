# Changelog

このファイルは git-cliff（cliff.toml）が生成する。手で編集しない。

## [0.1.52](https://github.com/koyori-app/task/compare/v0.1.51...v0.1.52) - 2026-11-17

タスクに種別を持たせ、一覧の検索を条件式の絞り込みにまとめ、アカウント設定にテーマのページを、予定の API に期間の無いタスクを引く口を、CLI に作成時のキーと URL・一覧の項目選択・テナントの名前指定・更新の通知・コメントの読み出しを足し、畳んだサイドバーで開いているプロジェクトとその中の項目を帯で囲み、アカウント設定のナビをサイドバーと同じ部品に揃え、テナント削除と一括失効・利用者単位の取り消しのロック順とガントの日の段の読みにくさを直した

### 機能追加

- **backend**: 期間の無いタスクを引く口を予定の API に足す ([TASK-510](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-510)) ([#1184](https://github.com/koyori-app/task/pull/1184))
- **frontend**: アカウント設定にテーマのページを足し、配色を切り替えられるようにする ([TASK-382](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-382)) ([#1182](https://github.com/koyori-app/task/pull/1182))
- **frontend**: ガントチャートの画面を作り直し、日・週・月の切り替えと期間の無いタスクの一覧を足す ([TASK-510](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-510)) ([#1183](https://github.com/koyori-app/task/pull/1183))
- **frontend**: タスクの説明のチェックボックスを閲覧表示のまま切り替えられるようにする ([TASK-539](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-539)) ([#1198](https://github.com/koyori-app/task/pull/1198))
- タスクに種別を持たせる ([TASK-509](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-509)) ([#1199](https://github.com/koyori-app/task/pull/1199))
- **cli**: 作成のキー・一覧の項目選択・テナントの名前指定・更新の通知・コメントの読み出しを足す ([TASK-533](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-533))（[TASK-489](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-489)、[TASK-523](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-523) を解決） ([#1187](https://github.com/koyori-app/task/pull/1187))
- **review**: 出し直した PR へ未解決の指摘を写して引き継げるようにする ([TASK-495](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-495)) ([#1188](https://github.com/koyori-app/task/pull/1188))
- **frontend**: 説明とコメントの KEY-N のリンクにタスクの概要のカードを出す ([TASK-534](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-534)) ([#1190](https://github.com/koyori-app/task/pull/1190))
- 一覧の検索を条件式の絞り込みにまとめる ([TASK-500](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-500)) ([#1192](https://github.com/koyori-app/task/pull/1192))
- **frontend**: 畳んだサイドバーで、開いているプロジェクトとその中の項目を帯で囲む ([#1200](https://github.com/koyori-app/task/pull/1200))

### 修正

- **gantt**: 日の段の日の数字と範囲の外へ続く印を読めるようにする ([TASK-510](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-510)) ([#1193](https://github.com/koyori-app/task/pull/1193))
- **backend**: テナント削除と一括失効・利用者単位の取り消しのロック順をそろえる ([TASK-529](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-529)) ([#1186](https://github.com/koyori-app/task/pull/1186))
- **frontend**: 権限の無い操作を出さず、拒否されたら権限不足と明記する ([TASK-461](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-461)) ([#1189](https://github.com/koyori-app/task/pull/1189))

### リファクタリング

- **frontend**: アカウント設定のナビをサイドバーと同じ SidebarLink に揃える ([TASK-549](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-549)) ([#1202](https://github.com/koyori-app/task/pull/1202))
## [0.1.51](https://github.com/koyori-app/task/compare/v0.1.50...v0.1.51) - 2026-11-15

アクセストークンを複数のテナントに結び付けられるようにし、プロジェクトのメンバーをテナントから継承する形に改め、担当者全員の完了で完了になるタスク・未解決の指摘がある PR へのレビュワー名義の変更要求・担当者なしでの絞り込み・サブタスク作成時の日付の引き継ぎ・アカウント設定での 2 段階認証の管理・管理者向けのユーザー認証復旧と一般登録ポリシーの画面・説明欄のチェックリスト達成率と外部リンクの favicon・メンションのホバーカード・List 表示でサブタスクを最初から展開する設定を足し、実験的機能を表示するだけでガントが出る不具合・dialog 全体のスクロール・レビュー画面の PR 一覧の初期表示を直した

### 機能追加

- **frontend**: List 表示でサブタスクを最初から展開する設定を足す ([TASK-348](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-348)) ([#1151](https://github.com/koyori-app/task/pull/1151))
- **frontend**: メンションの札にプロフィールのホバーカードを出す ([TASK-512](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-512)) ([#1168](https://github.com/koyori-app/task/pull/1168))
- 管理者向けのユーザー認証復旧と一般登録ポリシーの画面を追加する ([TASK-191](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-191)) ([#1172](https://github.com/koyori-app/task/pull/1172))
- 担当者全員が自分の分を完了にして初めて完了になるタスク ([TASK-494](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-494)) ([#1175](https://github.com/koyori-app/task/pull/1175))
- 説明欄の外部リンクの横にサイトの favicon を出す ([TASK-362](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-362)) ([#1152](https://github.com/koyori-app/task/pull/1152))
- **frontend**: アカウント設定で 2 段階認証の有効化・無効化とリカバリーコードの再生成をできるようにする ([TASK-188](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-188)) ([#1170](https://github.com/koyori-app/task/pull/1170))
- **backend**: サブタスク作成時に親の開始予定日・締切を初期値にする ([TASK-506](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-506)) ([#1173](https://github.com/koyori-app/task/pull/1173))
- **frontend**: 説明欄のチェックリスト達成率を List 表示と詳細に出す ([TASK-519](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-519)) ([#1174](https://github.com/koyori-app/task/pull/1174))
- **backend**: タスク一覧を担当者なしで絞り込めるようにする ([TASK-498](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-498)) ([#1177](https://github.com/koyori-app/task/pull/1177))
- **cli**: task my list にページングと未完了だけの絞り込みを足す ([TASK-493](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-493)) ([#1178](https://github.com/koyori-app/task/pull/1178))
- **backend**: 未解決の指摘がある PR にレビュワー本人の名義で変更要求を出す ([TASK-518](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-518)) ([#1179](https://github.com/koyori-app/task/pull/1179))
- アクセストークンを複数のテナントに結び付けられるようにする ([TASK-491](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-491)) ([#1180](https://github.com/koyori-app/task/pull/1180))

### 修正

- **frontend**: レビュー画面の PR 一覧を、開いた直後は Open の PR だけにする ([TASK-503](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-503)) ([#1158](https://github.com/koyori-app/task/pull/1158))
- **frontend**: タスク詳細などの dialog 全体がスクロールしてしまうのを直す ([TASK-505](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-505)) ([#1159](https://github.com/koyori-app/task/pull/1159))
- **frontend**: 実験的機能を表示するだけでガントが出る不具合を直す ([TASK-517](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-517)) ([#1171](https://github.com/koyori-app/task/pull/1171))
- プロジェクトのメンバーをテナントから継承する形にする ([TASK-501](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-501)) ([#1156](https://github.com/koyori-app/task/pull/1156))
## [0.1.50](https://github.com/koyori-app/task/compare/v0.1.49...v0.1.50) - 2026-11-13

タスクのブロッキング関係の設定・表示とガントの関係を一度で引く API を足し、タスク詳細の応答に添付ファイルを、プロフィール API にテナントとの関わり方を、CLI に projects labels を足し、タスク作成ダイアログを詳細画面の見た目に揃え、メンションを koyori の Mention に載せ替え、PAT の最終使用日時の記録・タスク保存時の更新日の反映・テナントメンバー除外と降格の確認・ヘッダーの歯車の行き先・開発サーバーの SSR 500・BOT の役の読み取りを直し、表示専用のラベルページを削除した

### 機能追加

- **frontend**: タスクのブロッキング関係を設定・表示できるようにする ([TASK-344](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-344)) ([#1149](https://github.com/koyori-app/task/pull/1149))
- **cli**: projects labels でプロジェクトのラベル一覧を出す ([TASK-496](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-496)) ([#1150](https://github.com/koyori-app/task/pull/1150))
- **backend**: タスク詳細の応答に添付ファイルを含める ([TASK-507](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-507)) ([#1161](https://github.com/koyori-app/task/pull/1161))
- **frontend**: タスク作成ダイアログを 1 列 + チップにし、詳細画面の見た目に揃える ([TASK-508](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-508)) ([#1164](https://github.com/koyori-app/task/pull/1164))
- **backend**: プロフィール API にテナントとの関わり方（membership）を足す ([TASK-515](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-515)) ([#1167](https://github.com/koyori-app/task/pull/1167))
- **backend**: ガントの課題同士の関係を一度で引く口を足す ([TASK-510](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-510)) ([#1163](https://github.com/koyori-app/task/pull/1163))
- **frontend**: メンションの札を koyori の Mention の固定 class に載せ替える ([TASK-511](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-511)) ([#1166](https://github.com/koyori-app/task/pull/1166))

### 修正

- **frontend**: タスクの保存で更新日を反映し、全項目に保存完了を出す ([TASK-156](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-156)) ([#1120](https://github.com/koyori-app/task/pull/1120))
- **frontend**: テナントメンバーの除外と自分の降格に確認を挟む ([TASK-459](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-459)) ([#1124](https://github.com/koyori-app/task/pull/1124))
- **backend**: BOT の応答の役を、知らない値でも読める形で包む ([TASK-502](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-502)) ([#1157](https://github.com/koyori-app/task/pull/1157))
- **frontend**: 開発サーバーで SSR が 500 になるのを直す ([TASK-513](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-513)) ([#1165](https://github.com/koyori-app/task/pull/1165))
- **backend**: PAT の最終使用日時を記録する ([TASK-492](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-492)) ([#1162](https://github.com/koyori-app/task/pull/1162))
- **frontend**: ヘッダーの歯車をテナント設定ではなくアカウント設定へ向ける ([TASK-388](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-388)) ([#1169](https://github.com/koyori-app/task/pull/1169))
## [0.1.49](https://github.com/koyori-app/task/compare/v0.1.48...v0.1.49) - 2026-11-11

ホームでプロジェクトが無いときの案内と作ったタスクへのリンクを出し、タスク詳細オーバーレイに閉じるボタンを付け、マージ可でなくなった PR の Approve を取り消して解消後に承認し直すようにし、本番の compose に backend の環境変数を通した

### 修正

- **backend**: マージ可でなくなった PR の Approve を取り消し、解消後に承認し直す ([TASK-253](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-253)) ([#1114](https://github.com/koyori-app/task/pull/1114))
- **frontend**: タスク詳細オーバーレイに閉じるボタンを出す ([TASK-460](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-460)) ([#1121](https://github.com/koyori-app/task/pull/1121))
- **frontend**: プロジェクトが無いホームに作成・参加の案内を出す ([TASK-457](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-457)) ([#1122](https://github.com/koyori-app/task/pull/1122))
- **frontend**: ホームで作ったタスクへのリンクを出し、担当者の初期値を自分にする ([TASK-458](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-458)) ([#1123](https://github.com/koyori-app/task/pull/1123))
## [0.1.48](https://github.com/koyori-app/task/compare/v0.1.47...v0.1.48) - 2026-11-08

テナントの BOT アカウントとプロジェクトごとのガントチャート、タスクの一括作成（API と CLI）を足し、List 表示でタスク ID をコピーできるようにして、スマホのタスク一覧とメンション候補の表示崩れを直した

### 機能追加

- **frontend**: List 表示でタスク ID を表示してコピーできるようにする ([TASK-485](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-485)) ([#1105](https://github.com/koyori-app/task/pull/1105))
- **backend**: タスクの一括作成エンドポイントを追加する ([TASK-480](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-480)) ([#1106](https://github.com/koyori-app/task/pull/1106))
- **cli**: tasks bulk-create を追加する ([TASK-480](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-480)) ([#1107](https://github.com/koyori-app/task/pull/1107))
- テナントが持つ BOT アカウントを足す ([TASK-407](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-407)) ([#1088](https://github.com/koyori-app/task/pull/1088))
- **frontend**: プロジェクトごとのガントチャートを足す ([#1116](https://github.com/koyori-app/task/pull/1116))

### 修正

- **frontend**: スマホのタスク一覧でタイトルが見えないのを直す ([TASK-484](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-484)) ([#1104](https://github.com/koyori-app/task/pull/1104))
- **frontend**: IME の変換確定の Enter や取り消しの Esc で画面の操作が動く問題を直す ([TASK-490](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-490)) ([#1113](https://github.com/koyori-app/task/pull/1113))
- **frontend**: メンションの候補が欄の枠に切られないようにする ([TASK-412](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-412)) ([#1117](https://github.com/koyori-app/task/pull/1117))

### その他

- Revert "chore: mermaid を 11.16.1 で凍結する" ([#1118](https://github.com/koyori-app/task/pull/1118))
## [0.1.47](https://github.com/koyori-app/task/compare/v0.1.46...v0.1.47) - 2026-11-06

Sentry にアプリエラー・パフォーマンス・ジョブの失敗を送るようにしてブラウザ側の初期化を配線し、人の PAT の発行でテナントの 2FA 強制を確かめるようにした

### 機能追加

- Sentry にアプリエラー・パフォーマンス・ジョブ失敗を送り、フロントの初期化を配線する ([TASK-483](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-483)) ([#1108](https://github.com/koyori-app/task/pull/1108))

### 修正

- **backend**: 人の PAT の発行口でテナントの 2FA 強制を確かめる ([TASK-481](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-481)) ([#1102](https://github.com/koyori-app/task/pull/1102))
## [0.1.46](https://github.com/koyori-app/task/compare/v0.1.45...v0.1.46) - 2026-11-05

他の人のタスク変更を再読み込みなしで画面に反映するようにし、サインイン後に元のページへ戻る・/v1/auth/me の一時的な失敗でサインイン画面へ飛ばさない・読み込みの失敗を再試行できるようにし、タスクの説明とコメントとレビューの入力欄で下書きを捨てないよう直し、My Tasks の期限判定をホームに揃え、プロジェクトキーの一意をテナント単位に直し、ドキュメントの書き方の指針を足して既存ページを書き直した

### 機能追加

- 他の人のタスク変更を再読み込みなしで画面に反映する ([TASK-138](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-138)) ([#1077](https://github.com/koyori-app/task/pull/1077))

### 修正

- **frontend**: 読み込みの失敗を「なし」「見つからない」と分け、再試行できるようにする ([TASK-452](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-452)) ([#1094](https://github.com/koyori-app/task/pull/1094))
- **frontend**: /v1/auth/me の一時的な失敗でサインイン画面へ飛ばさない ([TASK-454](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-454)) ([#1096](https://github.com/koyori-app/task/pull/1096))
- **frontend**: 認証ガード外の画面で一時的な失敗を未ログイン扱いしない ([TASK-454](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-454)) ([#1099](https://github.com/koyori-app/task/pull/1099))
- **frontend**: インライン編集の保存に失敗しても入力した下書きを残す ([TASK-250](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-250)) ([#1043](https://github.com/koyori-app/task/pull/1043))
- **backend**: プロジェクトキーの一意をテナント単位に直す ([#1086](https://github.com/koyori-app/task/pull/1086))
- **frontend**: コメントの下書きをタスクごとに持ち、移動しても失わない ([TASK-448](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-448)) ([#1092](https://github.com/koyori-app/task/pull/1092))
- **frontend**: タスクの説明の編集に保存とキャンセルのボタンを置き、破棄とクリアは確認を挟む ([TASK-450](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-450)) ([#1093](https://github.com/koyori-app/task/pull/1093))
- **frontend**: レビューの入力欄に書いた指摘が確定で捨てられないようにする ([TASK-451](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-451)) ([#1095](https://github.com/koyori-app/task/pull/1095))
- **frontend**: サインイン後に元のページへ戻る ([TASK-455](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-455)) ([#1097](https://github.com/koyori-app/task/pull/1097))
- My Tasks の期限判定をホームに揃え、完了済みを除く ([TASK-453](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-453)) ([#1098](https://github.com/koyori-app/task/pull/1098))

### リファクタリング

- **backend**: タスク作成の本体を service に下ろす ([TASK-480](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-480)) ([#1103](https://github.com/koyori-app/task/pull/1103))

### ドキュメント

- ドキュメントの書き方の指針と検査を追加する ([TASK-442](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-442)) ([#1082](https://github.com/koyori-app/task/pull/1082))
- 既存のドキュメントを STYLE.md の指針に合わせて書き直し、dev/ の下へ移す ([TASK-446](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-446)) ([#1083](https://github.com/koyori-app/task/pull/1083))
## [0.1.45](https://github.com/koyori-app/task/compare/v0.1.44...v0.1.45) - 2026-11-05

コメントとタスクの概要に絵文字のリアクションを付けられるようにし、アクティビティ欄を 1 本のタイムラインにまとめ、2FA のログイン継続と強制セットアップの画面を足したほか、新規タスクの入力を確認なしに捨てないようにし、タスク一覧の D&D・Safari の絵文字の字の切り替え・テナントオーナーの所属設定・テナント削除とプロジェクト作成の 500 を直し、本番で Sentry の DSN を backend / frontend それぞれに渡すようにした

### 機能追加

- **frontend**: アクティビティ欄を 1 本のタイムラインにする ([TASK-440](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-440)) ([#1074](https://github.com/koyori-app/task/pull/1074))
- コメントとタスクの概要に絵文字のリアクションを付けられるようにする ([TASK-445](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-445)) ([#1084](https://github.com/koyori-app/task/pull/1084))
- **frontend**: 2FA のログイン継続と強制セットアップの画面を追加 ([TASK-187](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-187)) ([#1090](https://github.com/koyori-app/task/pull/1090))
- **deploy**: 本番で Sentry の DSN を backend / frontend それぞれに渡す ([#1100](https://github.com/koyori-app/task/pull/1100))

### 修正

- **frontend**: macOS の Safari でも絵文字の字の切り替えを効かせる ([TASK-410](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-410)) ([#1075](https://github.com/koyori-app/task/pull/1075))
- **frontend**: タスク一覧の D&D を初回の押下で掴めて群の全域へ落とせるようにする ([TASK-441](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-441)) ([#1076](https://github.com/koyori-app/task/pull/1076))
- **backend**: テナントオーナーが自分の所属を設定できるようにする ([TASK-443](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-443)) ([#1079](https://github.com/koyori-app/task/pull/1079))
- **backend**: 自動生成キーの衝突でプロジェクト作成が 500 になる不具合を直す ([#1087](https://github.com/koyori-app/task/pull/1087))
- **backend**: 管理者のテナント削除が監査ログの外部キー違反で 500 になるのを直す ([TASK-413](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-413)) ([#1089](https://github.com/koyori-app/task/pull/1089))
- **frontend**: 新規タスクの入力を確認なしに捨てない・似たタスクは別タブで開く ([TASK-449](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-449)) ([#1091](https://github.com/koyori-app/task/pull/1091))
## [0.1.44](https://github.com/koyori-app/task/compare/v0.1.43...v0.1.44) - 2026-11-04

ガントで担当者を選んでその人の課題を一つのガントに出せるようにし、アクティビティ欄のスクロールと履歴の文言を直したほか、仕様書を現在のコードに合わせて書き直して図を Mermaid と SVG にした

### 機能追加

- **frontend**: ガントで担当者を選んで、その課題を一つのガントに出す ([TASK-409](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-409)) ([#1072](https://github.com/koyori-app/task/pull/1072))

### 修正

- **frontend**: アクティビティ欄のスクロールと履歴の文言を直す ([TASK-438](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-438)) ([#1073](https://github.com/koyori-app/task/pull/1073))

### ドキュメント

- 仕様書を現在のコードに合わせて書き直し、図を Mermaid と SVG にする ([TASK-397](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-397)) ([#1071](https://github.com/koyori-app/task/pull/1071))
## [0.1.43](https://github.com/koyori-app/task/compare/v0.1.42...v0.1.43) - 2026-11-04

タスク詳細で親タスクを設定・変更・解除できるようにし、ドキュメントサイトを Retype から Nimbus へ置き換えた

### 機能追加

- **docs**: ドキュメントサイトを Retype から Nimbus へ置き換える ([TASK-408](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-408)) ([#1069](https://github.com/koyori-app/task/pull/1069))
- **frontend**: タスク詳細で親タスクを設定・変更・解除できるようにする ([TASK-343](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-343)) ([#1070](https://github.com/koyori-app/task/pull/1070))
## [0.1.42](https://github.com/koyori-app/task/compare/v0.1.41...v0.1.42) - 2026-11-04

カレンダーと同じ予定を棒で見るガントの頁を追加し、環境設定で絵文字の字（Noto Color Emoji・Twemoji）を選べるようにし、担当者欄で部署・チームの候補をまとめて追加できるようにしたほか、最終期限だけが期間に入るタスクがカレンダーに出ない不具合とサインイン画面の空の面・読めない文字色を修正し、E2E を Desktop Safari と iPhone の WebKit でも実行するようにした

### 機能追加

- **tasks**: 担当者欄で部署・チームの候補をまとめて追加できるようにする ([TASK-396](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-396)) ([#1062](https://github.com/koyori-app/task/pull/1062))
- **frontend**: 環境設定で絵文字の字を選べるようにする ([TASK-385](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-385)) ([#1064](https://github.com/koyori-app/task/pull/1064))
- **frontend**: 絵文字の字に Twemoji を選べるようにする ([TASK-385](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-385)) ([#1067](https://github.com/koyori-app/task/pull/1067))
- **frontend**: カレンダーと同じ予定を棒で見るガントの頁を足す ([#1066](https://github.com/koyori-app/task/pull/1066))

### 修正

- **schedule**: 最終期限だけが期間に入るタスクをカレンダーに出す ([TASK-401](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-401)) ([#1063](https://github.com/koyori-app/task/pull/1063))
- **frontend**: サインイン画面の空の面と読めない文字色を直す ([TASK-404](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-404)) ([#1065](https://github.com/koyori-app/task/pull/1065))

### リファクタリング

- **frontend**: テナントメンバーの所属入力を Input に置き換える ([#1060](https://github.com/koyori-app/task/pull/1060))
## [0.1.41](https://github.com/koyori-app/task/compare/v0.1.40...v0.1.41) - 2026-11-02

テナントの 2FA 強制を既存セッション・PAT・Device Token にも効かせ、2FA の試行回数を検証前に予約して同時リクエストで上限を越えられないようにし、Drive 公開共有 API にレートリミットを足し、S3 アップロードを長さ未知でも multipart で流し、deadline_soon 通知を 15 分ごとのバッチで送るようにしたほか、タスク説明文に 100,000 文字の上限を設け、CLI が読む列挙欄を OrUnknown に統一し、作成ダイアログの日付が送られない・タイトルリンクでオーバーレイ直後に遷移する・別タスクへ移っても楽観更新が残る・完了タスクを期限切れ扱いにする不具合を修正

### 機能追加

- **backend**: deadline_soon 通知を 15 分ごとのバッチで送る ([TASK-263](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-263)) ([#1041](https://github.com/koyori-app/task/pull/1041))

### 修正

- **backend**: テナントの 2FA 強制を既存セッション・PAT・Device Token にも効かせる ([TASK-377](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-377)) ([#1036](https://github.com/koyori-app/task/pull/1036))
- **backend**: 2FA の試行回数を検証前に予約して同時リクエストで上限を越えられなくする ([TASK-378](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-378)) ([#1039](https://github.com/koyori-app/task/pull/1039))
- **frontend**: 完了したタスクを期限切れとして扱わない ([TASK-384](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-384)) ([#1035](https://github.com/koyori-app/task/pull/1035))
- **backend**: セッション専用の口を Bearer で叩いたときの 401 を 403 にする ([TASK-260](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-260)) ([#1038](https://github.com/koyori-app/task/pull/1038))
- タスク説明文に 100,000 文字の上限を設け、超過本文は SSR で描画しない ([TASK-122](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-122)) ([#1042](https://github.com/koyori-app/task/pull/1042))
- **backend**: S3 アップロードを長さ未知でも multipart で流し、フィールド順序と実効上限を直す ([TASK-163](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-163)) ([#1044](https://github.com/koyori-app/task/pull/1044))
- **backend**: Drive 公開共有 API に IP × トークン指紋のレートリミットを足す ([TASK-167](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-167)) ([#1040](https://github.com/koyori-app/task/pull/1040))
- **frontend**: タイトルリンクで開くとオーバーレイの直後に詳細ページへ遷移するのを直す ([TASK-337](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-337)) ([#1055](https://github.com/koyori-app/task/pull/1055))
- **frontend**: タスク作成ダイアログで入力済みに見える日付と見積もりが送られないのを直す ([TASK-390](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-390)) ([#1056](https://github.com/koyori-app/task/pull/1056))
- **frontend**: 別のタスクへ移ったときに前のタスクの楽観更新を消す ([TASK-146](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-146)) ([#1057](https://github.com/koyori-app/task/pull/1057))

### リファクタリング

- **payload**: CLI が読む応答の列挙欄を OrUnknown にし、OpenAPI から漏れを落とす固定テストを足す ([TASK-293](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-293)) ([#1045](https://github.com/koyori-app/task/pull/1045))
## [0.1.40](https://github.com/koyori-app/task/compare/v0.1.39...v0.1.40) - 2026-10-29

Drive の本体取得で PAT のテナント・プロジェクト束縛を検査し、プロジェクトから外れたフォルダ作成者の共有操作を拒否するよう修正したほか、アカウント設定のサイドバーに選択中のテナントのナビを出し、本番レシピの backend に RUST_ENV=production を設定

### 修正

- **deploy**: 本番レシピの backend に RUST_ENV=production を設定する ([TASK-379](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-379)) ([#1031](https://github.com/koyori-app/task/pull/1031))
- **frontend**: アカウント設定でもサイドバーに選択中のテナントのナビを出す ([TASK-373](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-373)) ([#1032](https://github.com/koyori-app/task/pull/1032))
- **backend**: Drive 本体取得で PAT のテナント・プロジェクト束縛を検査する ([TASK-375](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-375)) ([#1033](https://github.com/koyori-app/task/pull/1033))
- **backend**: プロジェクトから外れたフォルダ作成者の共有操作を拒否する ([TASK-376](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-376)) ([#1034](https://github.com/koyori-app/task/pull/1034))
## [0.1.39](https://github.com/koyori-app/task/compare/v0.1.38...v0.1.39) - 2026-10-28

類似タスク候補 API と作成ダイアログの重複候補・タスク詳細の「似ているタスク」欄を追加し、DropdownMenu・Card・Separator を @koyori-app/ui-vue に置き換えて shadcn-vue の依存を外したほか、テナント Admin に共有プロジェクトの作成と管理を許可

### 機能追加

- **backend**: 類似タスク候補 API GET /tasks/similar を追加する ([TASK-321](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-321)) ([#977](https://github.com/koyori-app/task/pull/977))
- **backend**: テナント Admin に共有プロジェクトの作成と管理を許す ([TASK-372](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-372)) ([#1010](https://github.com/koyori-app/task/pull/1010))
- **frontend**: 作成ダイアログの重複候補とタスク詳細の「似ているタスク」欄を追加する ([TASK-322](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-322)) ([#978](https://github.com/koyori-app/task/pull/978))

### リファクタリング

- **frontend**: Dialog を @koyori-app/ui-vue に置き換える ([TASK-353](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-353)) ([#996](https://github.com/koyori-app/task/pull/996))
- **frontend**: DropdownMenu を @koyori-app/ui-vue の Dropdown に置き換える ([TASK-354](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-354)) ([#997](https://github.com/koyori-app/task/pull/997))
- **frontend**: タスク詳細オーバーレイの退場アニメーション待ちのタイマーを外す ([TASK-363](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-363)) ([#1005](https://github.com/koyori-app/task/pull/1005))
- **frontend**: Select と UserSelector を @koyori-app/ui-vue の Picker に置き換える ([TASK-355](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-355)) ([#998](https://github.com/koyori-app/task/pull/998))
- **frontend**: Checkbox / Switch を @koyori-app/ui-vue に置き換え、ui/label を削除する ([TASK-356](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-356)) ([#999](https://github.com/koyori-app/task/pull/999))
- **frontend**: サイドバーを @koyori-app/ui-vue の Sidebar / Drawer に置き換える ([TASK-357](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-357)) ([#1000](https://github.com/koyori-app/task/pull/1000))
- **frontend**: Skeleton / Tooltip / Avatar / Breadcrumb を @koyori-app/ui-vue に置き換える ([TASK-358](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-358)) ([#1001](https://github.com/koyori-app/task/pull/1001))
- **frontend**: タスク一覧の Table を @koyori-app/ui-vue の DataList に置き換える ([TASK-359](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-359)) ([#1002](https://github.com/koyori-app/task/pull/1002))
- **frontend**: タスク一覧の分割ペインを @koyori-app/ui-vue の SplitPane に置き換える ([TASK-360](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-360)) ([#1003](https://github.com/koyori-app/task/pull/1003))
- **frontend**: Card / Separator を koyori のクラスに置き換え、shadcn-vue の依存を外す ([TASK-361](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-361)) ([#1004](https://github.com/koyori-app/task/pull/1004))
## [0.1.38](https://github.com/koyori-app/task/compare/v0.1.37...v0.1.38) - 2026-10-26

フロントエンドに @koyori-app/ui-vue を導入して Tailwind のテーマをトークンに結び付け Field / Input / Textarea を置き換えたほか、apalis ワーカーを監督して heartbeat 失敗から自動復帰するように修正

### 機能追加

- **frontend**: @koyori-app/ui-vue を導入し、Tailwind のテーマをトークンに結び付ける ([TASK-350](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-350)) ([#992](https://github.com/koyori-app/task/pull/992))

### 修正

- **backend**: apalis ワーカーを監督して heartbeat 失敗から自動復帰させる ([#1009](https://github.com/koyori-app/task/pull/1009))

### リファクタリング

- **frontend**: Button を @koyori-app/ui-vue に置き換える ([TASK-351](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-351)) ([#993](https://github.com/koyori-app/task/pull/993))
- **frontend**: Field / Input / Textarea を @koyori-app/ui-vue に置き換える ([TASK-352](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-352)) ([#994](https://github.com/koyori-app/task/pull/994))
## [0.1.37](https://github.com/koyori-app/task/compare/v0.1.36...v0.1.37) - 2026-10-24

CLI の tenant_id を PAT から自動解決して設定を任意にし、TASK_API_URL と TASK_TOKEN を組で扱うように変更、中継が弱くした ETag を If-Match に戻すとドキュメント保存が 412 になる不具合を修正

### 機能追加

- **cli**: tenant_id を任意にし PAT から自動解決する ([TASK-367](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-367)) ([#1006](https://github.com/koyori-app/task/pull/1006))
- **破壊的変更** **cli**: TASK_API_URL と TASK_TOKEN を組で扱い片側だけの上書きを止める ([TASK-368](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-368)) ([#1007](https://github.com/koyori-app/task/pull/1007))

### 修正

- **backend**: 中継が弱くした ETag を If-Match に戻すとドキュメントの保存が常に 412 になるのを直す ([TASK-336](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-336)) ([#990](https://github.com/koyori-app/task/pull/990))
## [0.1.36](https://github.com/koyori-app/task/compare/v0.1.35...v0.1.36) - 2026-10-22

テナントメンバーの部署・役職・チーム、タスク検索の @名 でその者をメンションした説明文も当てる検索、コメント本文の KFM での描画、メンションの札からプロフィールへのリンク、新しいアプリのアイコンとファビコンを追加

### 機能追加

- テナントメンバーに部署・役職・チームを持てるようにする ([TASK-334](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-334)) ([#989](https://github.com/koyori-app/task/pull/989))
- **backend**: タスク検索の @名 で、その者をメンションした説明文も当てる ([TASK-306](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-306)) ([#958](https://github.com/koyori-app/task/pull/958))
- **frontend**: コメントの本文を説明文と同じ KFM の描き手で描く ([TASK-308](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-308)) ([#959](https://github.com/koyori-app/task/pull/959))
- **frontend**: メンションの札をプロフィールへのリンクにする ([TASK-305](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-305)) ([#963](https://github.com/koyori-app/task/pull/963))
- **frontend**: アプリのアイコンとファビコンを差し替える ([TASK-299](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-299)) ([#991](https://github.com/koyori-app/task/pull/991))
## [0.1.35](https://github.com/koyori-app/task/compare/v0.1.34...v0.1.35) - 2026-10-21

プロジェクトのドキュメント一覧・詳細・編集画面、pg_trgm による日本語の曖昧検索、カレンダーの帯のドラッグとキーボードでの開始予定日の変更、T キーでのタスク作成、最後の Admin をプロジェクトから外して継承へ戻せる修正、作成したタスクの一覧への即時反映、自分宛ての assigned 通知の抑止を追加

### 機能追加

- **frontend**: プロジェクトのドキュメント一覧・詳細・編集画面を追加する ([#916](https://github.com/koyori-app/task/pull/916))
- **backend**: pg_trgm を導入し、タスク検索を日本語の部分一致と表記ゆれに対応させる ([TASK-320](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-320)) ([#976](https://github.com/koyori-app/task/pull/976))
- **frontend**: カレンダーの帯をドラッグ・キーボードで動かして開始予定日を変える ([TASK-313](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-313)) ([#970](https://github.com/koyori-app/task/pull/970))
- **frontend**: T でタスク作成モーダルを開くキーボードショートカットを足す ([TASK-332](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-332)) ([#987](https://github.com/koyori-app/task/pull/987))

### 修正

- **backend**: 自分を担当にしたときに自分宛ての assigned 通知が作られるのを直す ([TASK-280](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-280)) ([#985](https://github.com/koyori-app/task/pull/985))
- **frontend**: モーダルで作成したタスクを一覧にすぐ反映する ([TASK-333](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-333)) ([#986](https://github.com/koyori-app/task/pull/986))
- **backend**: 最後のメンバーである Admin をプロジェクトから外せるようにする ([TASK-217](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-217)) ([#988](https://github.com/koyori-app/task/pull/988))

### ドキュメント

- **backend**: タスクの曖昧検索・類似タスク候補（pg_trgm）の設計書を追加 ([TASK-319](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-319)) ([#975](https://github.com/koyori-app/task/pull/975))
## [0.1.34](https://github.com/koyori-app/task/compare/v0.1.33...v0.1.34) - 2026-10-21

環境設定と実験的機能の画面、利用者選択部品の作り直しとメンション札のアバター化、カレンダーの山積み表示と残工数の計算、Closes 付き PR のマージによるタスクの自動完了、Drive 本文からのタスク参照、schedule API の作業記録合計、Discord Webhook の見出しと色を追加

### 機能追加

- **backend**: Discord の Webhook の embed に読める見出しとイベントごとの色を付ける ([TASK-295](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-295)) ([#966](https://github.com/koyori-app/task/pull/966))
- **backend**: schedule API に作業記録の合計 logged_minutes と include_overdue を追加 ([TASK-310](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-310)) ([#967](https://github.com/koyori-app/task/pull/967))
- **backend**: Drive の Markdown 本文から KEY-N を抽出してタスクに結び付ける ([#914](https://github.com/koyori-app/task/pull/914))
- Closes 付き PR のマージでタスクを自動完了し、未適用の表示と確認済み操作を足す ([TASK-289](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-289)) ([#923](https://github.com/koyori-app/task/pull/923))
- **frontend**: カレンダーの見積/日を山の棒で表示する ([TASK-311](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-311)) ([#968](https://github.com/koyori-app/task/pull/968))
- **frontend**: 山積みを残工数・今日以降・担当者の均等割りで計算する ([TASK-312](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-312)) ([#969](https://github.com/koyori-app/task/pull/969))
- メンションの札をアバターと @名 の丸い札にする ([TASK-305](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-305)) ([#961](https://github.com/koyori-app/task/pull/961))
- **frontend**: 利用者の選択を一つの部品に作り直す ([TASK-307](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-307)) ([#960](https://github.com/koyori-app/task/pull/960))
- **frontend**: プロジェクトのメンバー追加で、テナントメンバーを UserSelector から選ぶ ([TASK-307](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-307)) ([#962](https://github.com/koyori-app/task/pull/962))
- **frontend**: 環境設定の画面と、既定で隠した実験的機能の画面を足す ([TASK-317](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-317), [TASK-318](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-318)) ([#973](https://github.com/koyori-app/task/pull/973))
## [0.1.33](https://github.com/koyori-app/task/compare/v0.1.32...v0.1.33) - 2026-10-20

タスクと PR・ブランチの連携（CLI のリンク操作・コミット集合の同期・レビューの結線）と取り込み元 Issue の表示を追加し、カレンダーのアバターと Drive のアップロード上限を直す

### 機能追加

- **backend**: Drive のフォルダ・ファイル一覧にプロジェクト・MIME・名前の絞り込みを足す ([#913](https://github.com/koyori-app/task/pull/913))
- タスク詳細に取り込み元 GitHub Issue の行を出す ([TASK-290](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-290)) ([#921](https://github.com/koyori-app/task/pull/921))
- レビューと PR 行の結線（PR の状態・未解決件数・pr_merged 通知） ([TASK-264](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-264)) ([#924](https://github.com/koyori-app/task/pull/924))
- **frontend**: カレンダーのメンバーの行に担当者のアバターを出す ([TASK-304](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-304)) ([#931](https://github.com/koyori-app/task/pull/931))
- **frontend**: オーバーレイの中の KEY-N とカレンダーのタスクをオーバーレイで開く ([TASK-303](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-303)) ([#932](https://github.com/koyori-app/task/pull/932))
- ブランチのリンクと PR のコミット集合の同期 ([TASK-291](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-291)) ([#925](https://github.com/koyori-app/task/pull/925))
- レビューの PR 一覧で承認待ちの指摘を未対応と分けて返す ([TASK-302](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-302)) ([#933](https://github.com/koyori-app/task/pull/933))
- **cli**: タスクと PR の連携（一覧・リンク・解除）を扱えるようにする ([TASK-314](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-314)) ([#965](https://github.com/koyori-app/task/pull/965))

### 修正

- **backend**: 統合テストが試験用と明示した DB と redis にだけ繋ぐようにする ([#957](https://github.com/koyori-app/task/pull/957))
- **backend**: Drive のアップロードが 2 MB を超えると 500 になる問題を直す ([#927](https://github.com/koyori-app/task/pull/927))
- **frontend**: カレンダーの行のアバターを UserAvatar に寄せ、担当者なしの行も同じ色の段にする ([TASK-316](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-316)) ([#972](https://github.com/koyori-app/task/pull/972))

### リファクタリング

- **frontend**: 利用者のアバターを UserAvatar 一つに寄せ、頭文字の色と大きさを揃える ([TASK-305](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-305)) ([#956](https://github.com/koyori-app/task/pull/956))
## [0.1.32](https://github.com/koyori-app/task/compare/v0.1.31...v0.1.32) - 2026-10-19

PR のタイトル・本文のタスク ID による自動リンクと、PR の手動リンク・解除を追加し、依存関係を更新する

### 機能追加

- PR のタイトル・本文のタスク ID から自動で関連付ける ([TASK-220](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-220)) ([#920](https://github.com/koyori-app/task/pull/920))
- PR の手動リンク・解除（API とダイアログ）([TASK-288](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-288)) ([#922](https://github.com/koyori-app/task/pull/922))
## [0.1.31](https://github.com/koyori-app/task/compare/v0.1.30...v0.1.31) - 2026-10-18

カレンダーのタイムライン・アバターのアップロード・メンション・PWA のインストールを追加し、GitLab self-hosted の OAuth を許可した instance に限る

### 機能追加

- **backend**: タスク一覧を作成者で絞り込めるようにする ([#907](https://github.com/koyori-app/task/pull/907))
- 他の利用者のプロフィールを閲覧できるようにする ([TASK-257](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-257)) ([#908](https://github.com/koyori-app/task/pull/908))
- **frontend**: KFM で KEY-N をタスク詳細への自動リンクにする ([#909](https://github.com/koyori-app/task/pull/909))
- **backend**: タスクに開始予定日 planned_start_on を追加する ([#910](https://github.com/koyori-app/task/pull/910))
- **backend**: 期間でタスク・スプリント・マイルストーンを返す schedule API を追加する ([#911](https://github.com/koyori-app/task/pull/911))
- **backend**: Drive ファイル本文に ETag を付け If-Match で楽観ロックする ([#912](https://github.com/koyori-app/task/pull/912))
- **frontend**: テナントカレンダーのタイムライン画面を追加する ([#915](https://github.com/koyori-app/task/pull/915))
- アバター画像をサーバーにアップロードできるようにする ([#917](https://github.com/koyori-app/task/pull/917))
- コメントのメンションを利用者IDの印で持ち、補完と札で扱う ([TASK-298](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-298)) ([#930](https://github.com/koyori-app/task/pull/930))
- **frontend**: web をアプリとして install できるようにする ([TASK-297](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-297)) ([#928](https://github.com/koyori-app/task/pull/928))

### 修正

- **backend**: GitLab self-hosted の OAuth を運用者の許可した instance に限り、登録の設定を OAuth にも効かせる ([#929](https://github.com/koyori-app/task/pull/929))
## [0.1.30](https://github.com/koyori-app/task/compare/v0.1.29...v0.1.30) - 2026-10-15

テナント招待・GitHub 上のレビュー承認・PAT のスコープを追加し、依存関係を更新する

### 機能追加

- レビュー画面の PR 一覧に「自分の PR だけ」とページ送りを入れる ([TASK-243](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-243)) ([#855](https://github.com/koyori-app/task/pull/855))
- PAT のスコープを api / read_api 方式へ整理する ([TASK-199](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-199)) ([#856](https://github.com/koyori-app/task/pull/856))
- **frontend**: サブタスクのオーバーレイを Esc・外側クリックで親タスクへ戻す ([TASK-245](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-245)) ([#877](https://github.com/koyori-app/task/pull/877))
- **frontend**: タスク詳細のインライン編集の保存時に「保存しました」を出す ([TASK-246](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-246)) ([#876](https://github.com/koyori-app/task/pull/876))
- **review**: 指摘の状態に修正中（fixing）を追加する ([TASK-240](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-240)) ([#847](https://github.com/koyori-app/task/pull/847))
- **backend**: レビューが完了した PR をレビュワー本人の GitHub アカウントで Approve する ([TASK-244](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-244)) ([#878](https://github.com/koyori-app/task/pull/878))
- テナントにメールで招待できるようにする ([TASK-184](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-184)) ([#854](https://github.com/koyori-app/task/pull/854))

### 修正

- **frontend**: スマホの List 表示で行の上からスクロールできないのを直す ([TASK-254](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-254)) ([#880](https://github.com/koyori-app/task/pull/880))
- **backend**: Desktop の Device Token で GET /v1/auth/me を呼べるようにする ([TASK-249](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-249)) ([#879](https://github.com/koyori-app/task/pull/879))
## [0.1.29](https://github.com/koyori-app/task/compare/v0.1.28...v0.1.29) - 2026-10-04

タスクの添付プレビューと List 表示のドラッグ移動を追加し、依存関係を更新する

### 機能追加

- **frontend**: List 表示でタスクをドラッグして別のステータスへ移せるようにする（[TASK-239](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-239) を解決） ([#844](https://github.com/koyori-app/task/pull/844))

### 修正

- **frontend**: List 表示でグループ最後の行の下線が消えていたのを直す ([#849](https://github.com/koyori-app/task/pull/849))
- **frontend**: DnD の保存中に届いた一覧で移した行を戻さない（[TASK-241](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-241) を解決） ([#848](https://github.com/koyori-app/task/pull/848))

### その他

- タスクにファイルを添付して画面内でプレビューできるようにする ([#846](https://github.com/koyori-app/task/pull/846))
## [0.1.28](https://github.com/koyori-app/task/compare/v0.1.27...v0.1.28) - 2026-09-28

レビュー操作のマージ判定を追加し、依存関係を更新する

### 機能追加

- **backend**: Koyori Desktop 用の Device Token 認証を追加する ([#782](https://github.com/koyori-app/task/pull/782))
- **backend**: 指摘の available_actions と集計の gate を返す ([#783](https://github.com/koyori-app/task/pull/783))
- **backend**: 通知一覧をカーソル化し kind 絞り込み・project 要約・保持期間を足す ([#785](https://github.com/koyori-app/task/pull/785))
## [0.1.27](https://github.com/koyori-app/task/compare/v0.1.26...v0.1.27) - 2026-09-28

Webhook と通知機能を追加し、依存関係を更新する

### 機能追加

- **backend**: レビューの起票と指摘の状態遷移を in-app 通知にする ([TASK-224](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-224)) ([#770](https://github.com/koyori-app/task/pull/770))
- **backend**: 通知の一覧・既読・設定を CLI から使えるようにする ([TASK-225](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-225)) ([#772](https://github.com/koyori-app/task/pull/772))
- **reviews**: レビュー指摘の PR 一覧を番号で絞れるようにする ([#788](https://github.com/koyori-app/task/pull/788))
- **backend**: 通知設定の email_events に従って通知メールを送る ([TASK-226](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-226)) ([#776](https://github.com/koyori-app/task/pull/776))
- **backend**: プロジェクトのイベントを外部 Webhook へ送る ([TASK-227](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-227)) ([#779](https://github.com/koyori-app/task/pull/779))
- **frontend**: プロジェクト設定に Webhook セクションを追加する ([TASK-230](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-230)) ([#780](https://github.com/koyori-app/task/pull/780))

### 修正

- **cli**: allow auth whoami without TASK_TENANT ([#786](https://github.com/koyori-app/task/pull/786))
- **storybook**: TaskDetail のコメント投稿 story で投稿完了を待つ ([#777](https://github.com/koyori-app/task/pull/777))

### 性能

- **backend**: ビルド高速化 — cargo-hakari 導入と統合テストの単一バイナリ化 ([#778](https://github.com/koyori-app/task/pull/778))
## [0.1.26](https://github.com/koyori-app/task/compare/v0.1.25...v0.1.26) - 2026-09-23

Markdown の表を読みやすくし、個人トークンの me 応答に tenant_id を返す

### 機能追加

- **personal-tokens**: me 応答にバインド先の tenant_id を返す（[TASK-222](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-222) を解決） ([#769](https://github.com/koyori-app/task/pull/769))

### 修正

- **kfm**: GFM 表の列に下限幅を置き、CJK の一字縦落ちを直す ([#774](https://github.com/koyori-app/task/pull/774))
## [0.1.25](https://github.com/koyori-app/task/compare/v0.1.24...v0.1.25) - 2026-09-22

ホーム画面をワークスペースダッシュボードに置き換え、マークダウン表示の横溢れを直す

### 機能追加

- ホームにワークスペースのダッシュボードを追加 ([#768](https://github.com/koyori-app/task/pull/768))

### 修正

- **frontend**: タスク作成モーダルが本番で 2 列にならない原因のルートコメントを除き、閉じるボタンを右上に置く ([#743](https://github.com/koyori-app/task/pull/743))
- **frontend**: 長い URL とインラインコードの横溢れを防ぐ ([#670](https://github.com/koyori-app/task/pull/670))
- **reviews**: 根の要素と並ぶ template コメントを除き、main の赤を消す ([#747](https://github.com/koyori-app/task/pull/747))

### ドキュメント

- **features**: Drive 仕様書を現在の実装に合わせて改訂する ([#676](https://github.com/koyori-app/task/pull/676))
## [0.1.24](https://github.com/koyori-app/task/compare/v0.1.23...v0.1.24) - 2026-09-16

トークンを作る口の認可を共用のテナント Admin 判定へ寄せる

### 修正

- **auth**: 鍵を作る口を共用の require_tenant_admin へ寄せ、GUI を揃える ([#741](https://github.com/koyori-app/task/pull/741))
## [0.1.23](https://github.com/koyori-app/task/compare/v0.1.22...v0.1.23) - 2026-09-16

レビュー画面の URL 共有・KFM 表示と PAT の自己識別を追加する

### 機能追加

- **auth**: PAT が己を名乗る口を建て、CLI の auth whoami を通す ([#740](https://github.com/koyori-app/task/pull/740))
- コミットメッセージの KEY-N でコミットをタスクにリンクする ([TASK-140](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-140)) ([#721](https://github.com/koyori-app/task/pull/721))
- **backend**: コミット作者を GitHub ログイン名で Task ユーザーに紐づける ([TASK-209](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-209)) ([#722](https://github.com/koyori-app/task/pull/722))
- **frontend**: レビュー指摘の画面で、見ているものを URL に載せる ([#745](https://github.com/koyori-app/task/pull/745))

### 修正

- **frontend**: レビュー指摘の本文を KFM で描く ([#744](https://github.com/koyori-app/task/pull/744))
## [0.1.22](https://github.com/koyori-app/task/compare/v0.1.21...v0.1.22) - 2026-09-14

GitHub Issue の同期をタスクのアクティビティに記録する

### 機能追加

- **tasks**: GitHub Issue同期をアクティビティに記録する ([TASK-201](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-201)) ([#717](https://github.com/koyori-app/task/pull/717))
## [0.1.21](https://github.com/koyori-app/task/compare/v0.1.20...v0.1.21) - 2026-09-14

タスク作成モーダルを 2 列に作り直し、タスク ID のコピーとパンくずの実データ表示を入れる

### 機能追加

- **frontend**: パンくずを実データから組み立てる ([#718](https://github.com/koyori-app/task/pull/718))
- **github**: 利用中の GitHub App インストールを別プロジェクトで再利用できるようにする ([#726](https://github.com/koyori-app/task/pull/726))
- **frontend**: タスク作成モーダルを本文とプロパティの 2 列にする ([TASK-216](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-216)) ([#725](https://github.com/koyori-app/task/pull/725))
- **frontend**: タスク詳細のタスクIDをクリックでコピーできるようにする ([#720](https://github.com/koyori-app/task/pull/720))
## [0.1.20](https://github.com/koyori-app/task/compare/v0.1.19...v0.1.20) - 2026-09-10

Git ホスティング↔タスク連携の仕様を固め、PAT に admin:project スコープを追加する

### 機能追加

- **backend**: PAT に admin:project スコープを追加 ([#716](https://github.com/koyori-app/task/pull/716))

### ドキュメント

- **features**: Git ホスティング↔タスク連携の仕様を PR 実体表 + ホスト中立モデルに改訂 ([#669](https://github.com/koyori-app/task/pull/669))
## [0.1.19](https://github.com/koyori-app/task/compare/v0.1.18...v0.1.19) - 2026-09-10

サブタスクと複数の完了ステータスを入れ、テナント設定とタスク一覧の並べ替えを整える

### 機能追加

- **workspace**: 完了ステータスを複数持てるようにする ([#714](https://github.com/koyori-app/task/pull/714))
- **frontend**: テナントの設定とメンバー管理の画面を足す ([#689](https://github.com/koyori-app/task/pull/689))

### 修正

- **frontend**: ヘッダーのスクロールとListの表示順を修正 ([#711](https://github.com/koyori-app/task/pull/711))

### その他

- [TASK-197](https://task.koyori.app/koyori/projects/TASK/tasks/TASK-197) サブタスクの作成と表示に対応 ([#715](https://github.com/koyori-app/task/pull/715))
- List表示の列ヘッダーからタスクを並べ替えられるようにする ([#712](https://github.com/koyori-app/task/pull/712))
## [0.1.18](https://github.com/koyori-app/task/compare/v0.1.17...v0.1.18) - 2026-09-08

認証方法を設定画面から管理できるようにし、担当者の指定と CLI のタスク操作を広げる

### 機能追加

- **cli**: タスクの一覧・検索・作成・更新で API の項目を扱えるようにする ([#687](https://github.com/koyori-app/task/pull/687))
- **frontend**: 作成モーダルで担当者を指定できるようにする ([#707](https://github.com/koyori-app/task/pull/707))
- **workspace**: パスワードと OAuth 連携を認証方法として設定画面から管理する ([#690](https://github.com/koyori-app/task/pull/690))
## [0.1.17](https://github.com/koyori-app/task/compare/v0.1.16...v0.1.17) - 2026-09-07

プロジェクトにだけ参加する客分を通し、Drive のプロジェクト境界と ACL を一貫させる

### 機能追加

- **backend**: project にだけ参加する客分（project-only guest）を一級で通す ([#688](https://github.com/koyori-app/task/pull/688))
- **backend**: テナントに入れずにプロジェクトへ直接招けるようにする ([#709](https://github.com/koyori-app/task/pull/709))

### 修正

- **backend**: Drive のプロジェクトフォルダ境界と ACL を一貫させる ([#679](https://github.com/koyori-app/task/pull/679))
- **frontend**: 作成モーダルを開き直したとき前回の成功表示が残るのを直す ([#663](https://github.com/koyori-app/task/pull/663))
## [0.1.16](https://github.com/koyori-app/task/compare/v0.1.15...v0.1.16) - 2026-09-06

タスク一覧に List 表示とコードブロックの装飾を足し、一覧のページングと CLI の候補表示を整える

### 機能追加

- **frontend**: タスク一覧の表示状態をリロード・戻りで保つ ([#680](https://github.com/koyori-app/task/pull/680))
- **frontend**: コードブロックへ帯・複写・行番号・横スクロールの枠を足す（[#683](https://github.com/koyori-app/task/issues/683) を解決） ([#684](https://github.com/koyori-app/task/pull/684))
- **frontend**: タスク一覧に List 表示を足し、詳細をオーバーレイにする ([#682](https://github.com/koyori-app/task/pull/682))
- **frontend**: テナントとアカウントを専用ヘッダーへ移し、設定のアイコンを分ける ([#674](https://github.com/koyori-app/task/pull/674))
- **cli**: 名前で指定する値の候補が分かるようにする ([#666](https://github.com/koyori-app/task/pull/666))

### 修正

- **backend**: Drive 公開共有 API の二重 prefix を直す ([#678](https://github.com/koyori-app/task/pull/678))
- **backend**: テナント所属欠落の 403 に理由を名指しさせる ([#685](https://github.com/koyori-app/task/pull/685))
- **frontend**: 分割ビューの説明を KFM で描画する ([#677](https://github.com/koyori-app/task/pull/677))
## [0.1.15](https://github.com/koyori-app/task/compare/v0.1.14...v0.1.15) - 2026-09-04

公開イメージに署名を添え、タスクの優先度編集とサイドバーの遷移を直す

### 機能追加

- **frontend**: サイドバーからレビューページへ行けるようにする ([#658](https://github.com/koyori-app/task/pull/658))
- **frontend**: 作成後のタスクの優先度を変えられるようにする ([#665](https://github.com/koyori-app/task/pull/665))
- **ci**: 公開するコンテナイメージへ cosign の keyless 署名を添える（[#673](https://github.com/koyori-app/task/issues/673) を解決） ([#675](https://github.com/koyori-app/task/pull/675))

### 修正

- **frontend**: 一覧のクリックが広い画面でも詳細ページへ飛ぶのを直す ([#668](https://github.com/koyori-app/task/pull/668))
- **frontend**: スマホでナビから遷移したらサイドバーを閉じる ([#664](https://github.com/koyori-app/task/pull/664))
- **backend**: PAT 発行の scopes に最小 1 件の検証を付ける（[#633](https://github.com/koyori-app/task/issues/633) を解決） ([#681](https://github.com/koyori-app/task/pull/681))
## [0.1.14](https://github.com/koyori-app/task/compare/v0.1.13...v0.1.14) - 2026-09-03

説明欄を markdown エディタにし、タスク一覧のタップ判定を直す

### 機能追加

- **cli**: リリース配布物へ cosign の keyless 署名を添える（[#653](https://github.com/koyori-app/task/issues/653) を解決） ([#672](https://github.com/koyori-app/task/pull/672))
- **cli**: タスクの本文をコマンドから渡せるようにする ([#661](https://github.com/koyori-app/task/pull/661))
- **frontend**: 説明欄を CodeMirror 製の markdown エディタにする ([#671](https://github.com/koyori-app/task/pull/671))

### 修正

- **frontend**: トークン発行画面にレビュー系スコープを出す ([#662](https://github.com/koyori-app/task/pull/662))
- **frontend**: タスク一覧のどこをタップしても一番下のタスクが開くのを直す ([#660](https://github.com/koyori-app/task/pull/660))
## [0.1.13](https://github.com/koyori-app/task/compare/v0.1.12...v0.1.13) - 2026-09-02

CLI を Rust 製に置き換え、API プロキシのリダイレクト追跡を直す

### 機能追加

- **cli**: CLI を Rust 製に置き換える（[#650](https://github.com/koyori-app/task/issues/650) を解決） ([#651](https://github.com/koyori-app/task/pull/651))
- **cli**: タグ push で CLI のバイナリをリリースする（[#649](https://github.com/koyori-app/task/issues/649) を解決） ([#654](https://github.com/koyori-app/task/pull/654))

### 修正

- **frontend**: API プロキシが backend のリダイレクトを追わないようにする ([#655](https://github.com/koyori-app/task/pull/655))
## [0.1.12](https://github.com/koyori-app/task/compare/v0.1.11...v0.1.12) - 2026-09-02

GitHub 連携のリポジトリ選択トークンを取り落とさないようにする

### 修正

- **frontend**: GitHub 連携のリポジトリ選択トークンを取り落とさないようにする ([#652](https://github.com/koyori-app/task/pull/652))
## [0.1.10](https://github.com/koyori-app/task/compare/v0.1.9...v0.1.10) - 2026-09-01

v0.1.10 レビュー指摘の CLI と画面を追加し、改行コードを LF に固定

- レビュー指摘の CLI（[#626](https://github.com/koyori-app/task/issues/626)。submit / list / rounds / resolve / summary。
  summary は未レビュー・未解決・HEAD 不一致・連携なしで非 0 終了するゲート）
- レビュー指摘の画面（[#627](https://github.com/koyori-app/task/issues/627)。PR 一覧・指摘一覧・状態遷移・ラウンドの起票。
  マージ可否は鮮度と連携の有無で降格する 6 値）
- 改行コードを LF に固定し既存ファイルを正規化（[#586](https://github.com/koyori-app/task/issues/586)。.gitattributes と
  .git-blame-ignore-revs を追加。内容は不変）

### 機能追加

- レビュー指摘の CLI コマンドを追加する ([#626](https://github.com/koyori-app/task/pull/626))
- レビュー指摘の画面を追加する ([#627](https://github.com/koyori-app/task/pull/627))
## [0.1.9](https://github.com/koyori-app/task/compare/v0.1.8...v0.1.9) - 2026-08-31

v0.1.9 レビュー指摘機能・プロジェクトメンバー管理・パーソナルアクセストークンを追加

- レビュー指摘の基盤（[#623](https://github.com/koyori-app/task/issues/623) / [#624](https://github.com/koyori-app/task/issues/624) / [#625](https://github.com/koyori-app/task/issues/625)。仕様・API・GitHub 要約コメントの更新ジョブ）
- プロジェクトメンバー管理 UI（[#620](https://github.com/koyori-app/task/issues/620)。最後の Admin を守る直列化込み）
- パーソナルアクセストークンの発行・一覧（[#566](https://github.com/koyori-app/task/issues/566) / [#618](https://github.com/koyori-app/task/issues/618)）
- 同じ GitHub インストールを複数プロジェクトで使えるように（[#594](https://github.com/koyori-app/task/issues/594) / [#595](https://github.com/koyori-app/task/issues/595)）
- OpenAPI の operationId の一意性を修正（[#621](https://github.com/koyori-app/task/issues/621)）
- Forgejo CI の整備（[#631](https://github.com/koyori-app/task/issues/631)）

### 機能追加

- **github**: 連携するリポジトリをインストール後に選べるようにする（[#594](https://github.com/koyori-app/task/issues/594) を解決） ([#595](https://github.com/koyori-app/task/pull/595))
- パーソナルアクセストークンの発行画面を追加する ([#566](https://github.com/koyori-app/task/issues/566)) ([#618](https://github.com/koyori-app/task/pull/618))
- レビュー指摘管理の API を実装する ([#624](https://github.com/koyori-app/task/pull/624))
- レビュー要約を PR コメントへ反映するジョブを追加する ([#625](https://github.com/koyori-app/task/pull/625))
- プロジェクトメンバー管理 UI を実装する ([#317](https://github.com/koyori-app/task/issues/317)) ([#620](https://github.com/koyori-app/task/pull/620))

### 修正

- OpenAPI の operationId 重複を解消し、重複検知テストを追加する ([#621](https://github.com/koyori-app/task/pull/621))

### ドキュメント

- レビュー指摘管理の仕様書を追加する ([#623](https://github.com/koyori-app/task/pull/623))
## [0.1.8](https://github.com/koyori-app/task/compare/v0.1.7...v0.1.8) - 2026-08-27

v0.1.8 プロフィール編集・タスクコメント・KFM 本文表示・GitHub Issue 取り込み UI を追加

- プロフィール編集画面（[#581](https://github.com/koyori-app/task/issues/581)）
- タスク詳細のコメント UI（[#597](https://github.com/koyori-app/task/issues/597)）
- 課題本文の KFM 表示（[#580](https://github.com/koyori-app/task/issues/580) / [#583](https://github.com/koyori-app/task/issues/583) / [#596](https://github.com/koyori-app/task/issues/596)。GFM・alerts・コード着色・mermaid）
- GitHub Issue の取り込みを連携セクションから開始（[#590](https://github.com/koyori-app/task/issues/590)）
- タスク一覧のラベル列と作成ダイアログのラベル選択（[#538](https://github.com/koyori-app/task/issues/538)）

### 機能追加

- **backend**: ラベル変更の labels_changed アクティビティ記録 ([#536](https://github.com/koyori-app/task/pull/536))
- **backend**: 一括更新に remove_label_ids を追加 ([#537](https://github.com/koyori-app/task/pull/537))
- **frontend**: 一覧のラベル列と作成ダイアログのラベル選択 ([#538](https://github.com/koyori-app/task/pull/538))
- **frontend**: KFM Phase 1 レンダラ（GFM・alerts・安全 core） ([#580](https://github.com/koyori-app/task/pull/580))
- **frontend**: KFM のコードブロックを starry-night で着色する ([#584](https://github.com/koyori-app/task/pull/584))
- **frontend**: KFM に mermaid の図を入れる（client 描画・状態で待てる） ([#587](https://github.com/koyori-app/task/pull/587))
- **frontend**: 課題本文を KFM で描画する ([#596](https://github.com/koyori-app/task/pull/596))
- **frontend**: タスク詳細にコメント UI を追加する ([#597](https://github.com/koyori-app/task/pull/597))
- **frontend**: GitHub Issue の取り込みを連携セクションから開始できるようにする（[#589](https://github.com/koyori-app/task/issues/589) を解決） ([#590](https://github.com/koyori-app/task/pull/590))
- **frontend**: プロフィール編集画面を追加する（[#576](https://github.com/koyori-app/task/issues/576) を解決） ([#581](https://github.com/koyori-app/task/pull/581))

### 修正

- **frontend**: scope 付き描画で footnote-label も scope 付きにする ([#588](https://github.com/koyori-app/task/pull/588))
- **frontend**: CSS 契約の免除を機械照合へ変える ([#601](https://github.com/koyori-app/task/pull/601))
## [0.1.6](https://github.com/koyori-app/task/compare/v0.1.5...v0.1.6) - 2026-08-20

v0.1.6

### 機能追加

- タスクへのラベル付与（詳細画面での表示・付け外し） ([#534](https://github.com/koyori-app/task/pull/534))
- タスク一覧のラベルフィルタ ([#535](https://github.com/koyori-app/task/pull/535))
- **backend**: GitHub Issue の継続インポートと双方向同期を追加する（[#570](https://github.com/koyori-app/task/issues/570) を解決） ([#578](https://github.com/koyori-app/task/pull/578))

### 修正

- **frontend**: サインインのメール形式検証を入力に追従させる（[#571](https://github.com/koyori-app/task/issues/571) を解決） ([#574](https://github.com/koyori-app/task/pull/574))

### ドキュメント

- フロントエンドを Vike + Vue と正しく書く ([#582](https://github.com/koyori-app/task/pull/582))
## [0.1.4](https://github.com/koyori-app/task/compare/v0.1.3...v0.1.4) - 2026-08-19

v0.1.4 テナントメンバー機能を追加

### 機能追加

- **backend**: テナントにメンバーを持たせる（[#568](https://github.com/koyori-app/task/issues/568)、[#572](https://github.com/koyori-app/task/issues/572) を解決） ([#575](https://github.com/koyori-app/task/pull/575))
## [0.1.0](https://github.com/koyori-app/task/releases/tag/v0.1.0) - 2026-08-18

v0.1.0 初回リリース

### 機能追加

- バックエンドにSentryをセットアップ ([#7](https://github.com/koyori-app/task/pull/7))
- **backend**: 認証周りの実装 ([#5](https://github.com/koyori-app/task/pull/5))
- **backend**: support smtp ([#11](https://github.com/koyori-app/task/pull/11))
- **backend**: メール認証を行えるように ([#13](https://github.com/koyori-app/task/pull/13))
- add renovate ([#20](https://github.com/koyori-app/task/pull/20))
- **backend**: プロジェクトとテナントを追加 ([#19](https://github.com/koyori-app/task/pull/19))
- **front-vike**: vike版フロントエンド ([#4](https://github.com/koyori-app/task/pull/4))
- **backend**: PAT ([#25](https://github.com/koyori-app/task/pull/25))
- **drive**: add drive feature (Phase A — backend) ([#32](https://github.com/koyori-app/task/pull/32))
- front auth pages ([#47](https://github.com/koyori-app/task/pull/47))
- **tasks**: タスクコア機能を実装 ([#1](https://github.com/koyori-app/task/issues/1)) ([#40](https://github.com/koyori-app/task/pull/40))
- **backend**: admin flag ([#41](https://github.com/koyori-app/task/pull/41))
- **auth**: implement OAuth login ([#43](https://github.com/koyori-app/task/pull/43))
- **auth**: implement 2FA (TOTP) ([#45](https://github.com/koyori-app/task/pull/45))
- **auth**: implement password reset ([#42](https://github.com/koyori-app/task/pull/42))
- **tasks**: implement GitHub App wave 0 ([#44](https://github.com/koyori-app/task/pull/44))
- **auth**: implement Passkeys (WebAuthn) ([#46](https://github.com/koyori-app/task/pull/46))
- **frontend**: add Vitest unit test project with happy-dom ([#75](https://github.com/koyori-app/task/pull/75))
- **tasks**: implement sprints ([#78](https://github.com/koyori-app/task/pull/78))
- add tanstack query ([#104](https://github.com/koyori-app/task/pull/104))
- **tasks**: implement collaboration ([#77](https://github.com/koyori-app/task/pull/77))
- **tasks**: implement time-tracking ([#76](https://github.com/koyori-app/task/pull/76))
- openapi vue query（[#112](https://github.com/koyori-app/task/issues/112) を解決） ([#110](https://github.com/koyori-app/task/pull/110))
- setup pinia ([#114](https://github.com/koyori-app/task/pull/114))
- **front**: auth forms ([#134](https://github.com/koyori-app/task/pull/134))
- **front**: add vitest coverage CI for pull requests ([#137](https://github.com/koyori-app/task/pull/137))
- **custom-fields**: カスタムフィールド機能実装 ([#108](https://github.com/koyori-app/task/pull/108))
- feat(notifications): 通知・ウォッチャー機能実装 ([#141](https://github.com/koyori-app/task/pull/141))
- **my-tasks**: My Tasks バックエンド実装 ([#142](https://github.com/koyori-app/task/pull/142))
- ログインとアカウント作成（[#136](https://github.com/koyori-app/task/issues/136) を解決） ([#155](https://github.com/koyori-app/task/pull/155))
- enhance task table view mock ([#157](https://github.com/koyori-app/task/pull/157))
- **frontend**: add TanStack Devtools DX spike ([#159](https://github.com/koyori-app/task/pull/159))
- **auth**: メール未認証時の案内画面を追加 ([#195](https://github.com/koyori-app/task/pull/195))
- **tasks**: search, bulk, saved views, and attachments ([#105](https://github.com/koyori-app/task/pull/105))
- precommit fmt workspace ([#203](https://github.com/koyori-app/task/pull/203))
- zxcvbn v4 migration（[#189](https://github.com/koyori-app/task/issues/189) を解決） ([#204](https://github.com/koyori-app/task/pull/204))
- vite devtools setup ([#245](https://github.com/koyori-app/task/pull/245))
- **backend**: TaskResponse にユーザー情報(作成者・担当者)を内蔵する（[#271](https://github.com/koyori-app/task/issues/271) を解決） ([#275](https://github.com/koyori-app/task/pull/275))
- **backend**: CSRF Origin検査でPAT経路をスキップする（[#17](https://github.com/koyori-app/task/issues/17) を解決） ([#280](https://github.com/koyori-app/task/pull/280))
- **frontend**: パスワード再設定フローの UI を実装する（[#284](https://github.com/koyori-app/task/issues/284) を解決） ([#285](https://github.com/koyori-app/task/pull/285))
- **frontend**: implement real API for task table page ([#244](https://github.com/koyori-app/task/pull/244))
- **frontend**: connect tenant switcher to API（[#305](https://github.com/koyori-app/task/issues/305) を解決） ([#332](https://github.com/koyori-app/task/pull/332))
- **frontend**: タスク詳細ハブ（増分1: 詳細表示・一覧遷移・状態変更）（[#307](https://github.com/koyori-app/task/issues/307)、[#308](https://github.com/koyori-app/task/issues/308)、[#310](https://github.com/koyori-app/task/issues/310) を解決） ([#337](https://github.com/koyori-app/task/pull/337))
- e2e foundation ([#111](https://github.com/koyori-app/task/pull/111))
- **frontend**: 登録成功表示にパスワード再設定への導線を追加する（[#135](https://github.com/koyori-app/task/issues/135) を解決） ([#345](https://github.com/koyori-app/task/pull/345))
- **frontend**: connect project nav to the projects API（[#306](https://github.com/koyori-app/task/issues/306) を解決） ([#349](https://github.com/koyori-app/task/pull/349))
- **frontend**: create tasks from the project view（[#309](https://github.com/koyori-app/task/issues/309) を解決） ([#350](https://github.com/koyori-app/task/pull/350))
- **frontend**: focus My Tasks on assigned tasks（[#359](https://github.com/koyori-app/task/issues/359) を解決） ([#360](https://github.com/koyori-app/task/pull/360))
- **frontend**: プロジェクトの作成・編集・削除（デザイン準拠のページ + サイドバー導線）（[#365](https://github.com/koyori-app/task/issues/365)、[#369](https://github.com/koyori-app/task/issues/369) を解決） ([#366](https://github.com/koyori-app/task/pull/366))
- **frontend**: edit and delete tasks from the detail hub（[#309](https://github.com/koyori-app/task/issues/309) を解決） ([#351](https://github.com/koyori-app/task/pull/351))
- **frontend**: サイドバーを整理しプロジェクト展開サブナビを導入（[#376](https://github.com/koyori-app/task/issues/376) を解決） ([#377](https://github.com/koyori-app/task/pull/377))
- **frontend**: connect server-side task search UI（[#316](https://github.com/koyori-app/task/issues/316) を解決） ([#355](https://github.com/koyori-app/task/pull/355))
- **frontend**: プロジェクト設定に連携セクション（GitHub 連携）を追加 ([#382](https://github.com/koyori-app/task/pull/382))
- **frontend**: プロジェクト設定にラベルセクションを追加 ([#383](https://github.com/koyori-app/task/pull/383))
- **frontend**: プロジェクト設定にカスタムフィールドセクションを追加 ([#384](https://github.com/koyori-app/task/pull/384))
- add workflow statuses editor to project settings (v1)（[#370](https://github.com/koyori-app/task/issues/370) を解決） ([#388](https://github.com/koyori-app/task/pull/388))
- **frontend**: タスク一覧に詳細の分割ビューを追加 ([#444](https://github.com/koyori-app/task/pull/444))
- **backend**: ドライブファイルの本文編集エンドポイントを追加する（[#445](https://github.com/koyori-app/task/issues/445) を解決） ([#447](https://github.com/koyori-app/task/pull/447))
- GitLab を用いた OAuth ログインを追加する（[#451](https://github.com/koyori-app/task/issues/451) を解決） ([#452](https://github.com/koyori-app/task/pull/452))

### 修正

- **front-vike**: fix static plugin and optimizeDeps followup ([#24](https://github.com/koyori-app/task/pull/24))
- **frontend**: Vite dev serverにAPIプロキシを追加してCORSを回避 ([#64](https://github.com/koyori-app/task/pull/64))
- **renovate**: group all vite-plus family packages together ([#74](https://github.com/koyori-app/task/pull/74))
- **auth**: Input の :value/:input を :model-value/:update:model-value に修正 ([#194](https://github.com/koyori-app/task/pull/194))
- **backend**: tasks-extensions レビュー指摘対応（FK整合・updated_at・ACLテスト） ([#199](https://github.com/koyori-app/task/pull/199))
- guard prehydration form submits ([#158](https://github.com/koyori-app/task/pull/158))
- **frontend**: resolve lint errors ([#239](https://github.com/koyori-app/task/pull/239))
- コードレビュー指摘の一括修正（board 認証・CSRF・プレースホルダバグ他） ([#272](https://github.com/koyori-app/task/pull/272))
- **backend**: メール認証トークンをジョブペイロードから排除し Redis のみに保持 ([#273](https://github.com/koyori-app/task/pull/273))
- **backend**: admin_tenants の ? プレースホルダ未変換を修正 ([#277](https://github.com/koyori-app/task/pull/277))
- **github**: derive_keyをHKDF-SHA256に置き換える（[#66](https://github.com/koyori-app/task/issues/66) を解決） ([#279](https://github.com/koyori-app/task/pull/279))
- **backend**: アカウント登録のemail enumerationを防ぐ（[#26](https://github.com/koyori-app/task/issues/26) を解決） ([#281](https://github.com/koyori-app/task/pull/281))
- **backend**: POST /v1/tenants の OpenAPI に 409 Conflict を宣言する（[#336](https://github.com/koyori-app/task/issues/336) を解決） ([#338](https://github.com/koyori-app/task/pull/338))
- **frontend**: マイタスクページで tenant display_id を UUID に解決する ([#357](https://github.com/koyori-app/task/pull/357))
- **backend**: create_project で既定ワークフローステータスを seed する（[#368](https://github.com/koyori-app/task/issues/368) を解決） ([#372](https://github.com/koyori-app/task/pull/372))
- **backend**: GitHub callback の戻り先を実在する frontend ルートに修正 ([#386](https://github.com/koyori-app/task/pull/386))
- **backend**: job クレートの sqlx 直接依存を撤廃する ([#446](https://github.com/koyori-app/task/pull/446))
- **storybook**: SignUpForm と SignInForm のストーリーが autosize で幅崩れするのを防ぐ ([#440](https://github.com/koyori-app/task/pull/440))
- **backend**: アップロードされた HTML が同一オリジンで実行される問題を修正 ([#510](https://github.com/koyori-app/task/pull/510))
- **backend**: セッション専用エクストラクタが Bearer 付きリクエストを拒否するよう統一 ([#532](https://github.com/koyori-app/task/pull/532))
- **backend**: テナントレベルのドライブファイルが未認証で取得できる問題を修正 ([#522](https://github.com/koyori-app/task/pull/522))
- **frontend**: サインインの 403 を一律メール未認証と誤判定する問題を修正 ([#533](https://github.com/koyori-app/task/pull/533))

### 性能

- **backend**: dev ビルド高速化 — Cargo profile チューニング / sccache / sea-orm 不要 feature 削除 ([#152](https://github.com/koyori-app/task/pull/152))
- **backend**: dev ビルド高速化 — profile チューニング・sccache 導入・sea-orm debug-print 削除 ([#154](https://github.com/koyori-app/task/pull/154))
- **storage**: aws-sdk-s3 を object_store に置き換えてビルド時間を短縮 ([#153](https://github.com/koyori-app/task/pull/153))

### リファクタリング

- **frontend**: openapi-generator-cli を openapi-typescript + openapi-fetch に移行 ([#101](https://github.com/koyori-app/task/pull/101))
- **front**: clientOnlyは非推奨関数なので、テンプレート側でClientOnlyを使うように ([#193](https://github.com/koyori-app/task/pull/193))
- move DTOs from handlers to dedicated payload module ([#201](https://github.com/koyori-app/task/pull/201))
- SeaORM エンティティをスキーマファースト運用へ移行 ([#202](https://github.com/koyori-app/task/pull/202))
- **backend**: entity を独立クレートへ分離 ([#151](https://github.com/koyori-app/task/issues/151) Phase 1) ([#242](https://github.com/koyori-app/task/pull/242))
- **workspace**: extract common crate ([#151](https://github.com/koyori-app/task/issues/151) Phase 2) ([#267](https://github.com/koyori-app/task/pull/267))
- **workspace**: payload クレートへ DTO を切り出し ([#151](https://github.com/koyori-app/task/issues/151) Phase 3) ([#276](https://github.com/koyori-app/task/pull/276))
- **workspace**: service / job / handler クレートへ分割 ([#151](https://github.com/koyori-app/task/issues/151) Phase 4) ([#278](https://github.com/koyori-app/task/pull/278))
- **backend**: quick-capture API を撤去（[#363](https://github.com/koyori-app/task/issues/363) を解決） ([#364](https://github.com/koyori-app/task/pull/364))
- **frontend**: migrate CreateTenantDialog to shadcn-vue dialog wrapper ([#378](https://github.com/koyori-app/task/pull/378))
- **backend**: GitHub 連携を github-integration クレートに切り出し ([#448](https://github.com/koyori-app/task/pull/448))
- **backend**: reorder_statuses をトランザクション＋FOR UPDATE ロック化し N+1 を解消 ([#450](https://github.com/koyori-app/task/pull/450))
- **backend**: OAuth と GitHub App 連携を auth-core クレート群へ切り出す ([#454](https://github.com/koyori-app/task/pull/454))

### ドキュメント

- 仕様書を追加 ([#38](https://github.com/koyori-app/task/pull/38))
- 実装順序・並行実行判断表を追加 ([#39](https://github.com/koyori-app/task/pull/39))
- ドキュメントをretypeに ([#48](https://github.com/koyori-app/task/pull/48))
