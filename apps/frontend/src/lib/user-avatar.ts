/**
 * 利用者のアバターの大きさの段。UserAvatar は丸の直径（size-*）を koyori の Avatar の size（px）に直して使い、
 * 頭文字の字の大きさは koyori が直径から決める。
 */
export const USER_AVATAR_SIZES = {
  /** 最小の段（16px） */
  xs: 'size-4 text-[8px]',
  sm: 'size-5 text-[9px]',
  md: 'size-6 text-[10px]',
  lg: 'size-7 text-[10px]',
  xl: 'size-8 text-xs',
  '2xl': 'size-14 text-base',
  '3xl': 'size-16 text-base',
} as const;

/**
 * 頭文字の色。地（bg-muted）に対する text-foreground は、明るい面でも暗い面でも対比が 4.5:1 を
 * 十分に超える。薄い字（text-muted-foreground）は明るい面で 4.47:1 しかなく、12px 以下の字には足りぬ。
 */
export const USER_AVATAR_INITIALS_CLASS = 'bg-muted text-foreground';

/** 外れた者（退会・プロジェクトの外）のアバターの灰色。画像も頭文字も灰色にして薄くする。 */
export const USER_AVATAR_INACTIVE_CLASS = 'opacity-75 grayscale';

/**
 * 画像の無いメンバーの一覧で、頭文字の地に使う色（白い字を載せる）。名から色を決め、同じ人はいつも同じ色になる。
 * どれも白に対して 4.5:1 以上であること（試験で縛る）。
 */
export const MEMBER_AVATAR_COLORS = [
  '#0f766e',
  '#7c3aed',
  '#b45309',
  '#be123c',
  '#1d4ed8',
  '#4d7c0f',
] as const;

export type UserAvatarSize = keyof typeof USER_AVATAR_SIZES;

/** アバターに要る利用者の情報。名は `name`（表示名）か `username` のどちらでもよい。 */
export interface UserAvatarUser {
  account_kind?: string;
  id?: string;
  name?: string | null;
  username?: string | null;
  avatar_url?: string | null;
}
