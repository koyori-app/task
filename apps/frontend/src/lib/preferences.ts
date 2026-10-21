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
} as const satisfies SettingTable;

export type PreferenceKey = keyof typeof PREFERENCE_TABLE;
