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
} as const satisfies SettingTable;

export type PreferenceKey = keyof typeof PREFERENCE_TABLE;
