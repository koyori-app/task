import { DEFAULT_EMOJI_FONT, EMOJI_FONT_CHOICES } from '@/lib/emoji-font';
import type { SettingTable } from '@/lib/local-settings';

/**
 * 環境設定の項目の表。この端末のこのブラウザだけで効く設定で、アカウントには紐づかない。
 * 項目を足す・消す時はここだけを直す（localStorage に残った、表に無い key は読み込む時に捨てる）。
 */
export const PREFERENCE_TABLE = {
  showExperiments: {
    name: '実験的機能を表示する',
    description:
      'アカウント設定の一覧に「実験的機能」を出します。不安定な機能を、公開の前に試せます。',
    default: false,
  },
  emojiFont: {
    name: '絵文字の字',
    description:
      '絵文字をどの字で描くかを選びます。OS の既定では、使っている OS の絵文字で描きます。数字や # などの記号は変わりません。',
    choices: EMOJI_FONT_CHOICES,
    default: DEFAULT_EMOJI_FONT,
  },
  expandSubtasksInList: {
    name: 'タスク一覧でサブタスクを最初から展開する',
    description:
      'プロジェクトのタスク一覧の List 表示で、親タスクの行の下にサブタスクを開いた状態で出します。行の矢印で個別に閉じられます。',
    default: false,
  },
  ganttProgressLine: {
    name: 'ガントに稲妻線を出す',
    description:
      'ガントに、今日の時点の進みを結ぶ線を出します。ガントの切り替えで変えた時も、ここに覚えます。',
    default: false,
  },
  ganttDependencies: {
    name: 'ガントに依存の矢印を出す',
    description:
      'ガントに、ブロックの関係を矢印で出します（狭い画面では行に依存の印を添えます）。ガントの切り替えで変えた時も、ここに覚えます。',
    default: true,
  },
  advanceDateRowOnPick: {
    name: 'タスクの作成で、日付を選んだら次の行へ進む',
    description:
      'タスクを作る画面の日付で、開始予定日を選ぶと期限の行へ、続けて期限を選ぶと最終期限の行へ進みます。',
    default: true,
  },
} as const satisfies SettingTable;

export type PreferenceKey = keyof typeof PREFERENCE_TABLE;
