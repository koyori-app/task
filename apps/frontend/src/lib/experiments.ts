import type { SettingTable } from '@/lib/local-settings';

/**
 * 実験的機能のフラグの表。既定はどれも無効。
 * 機能の側は `useExperimentFlag(key)`（@/composables/useLocalSettings）から読む。
 * フラグを足す・消す時はここだけを直す（localStorage に残った、表に無い key は読み込む時に捨てる）。
 */
export const EXPERIMENT_TABLE = {
  koyoriArcIntegration: {
    name: 'ガントチャート',
    description:
      'サイドバーにガントチャートへのリンクを出します。タスクの予定を棒で見られます。描画には開発中の koyori-arc を使います。',
    default: false,
  },
} as const satisfies SettingTable;

export type ExperimentKey = keyof typeof EXPERIMENT_TABLE;
