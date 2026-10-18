/** 利用者のアバターの大きさの段。丸の直径と、頭文字の字の大きさをここで決める。 */
export const USER_AVATAR_SIZES = {
  /** 文の行の中に置く札（メンション）用 */
  xs: 'size-4 text-[8px]',
  sm: 'size-5 text-[9px]',
  md: 'size-6 text-[10px]',
  lg: 'size-7 text-[10px]',
  xl: 'size-8 text-xs',
  /** 以下は頭文字の字の大きさを置く場所の字に任せる（プロフィール等の大きな表示） */
  '2xl': 'size-14',
  '3xl': 'size-16',
} as const;

export type UserAvatarSize = keyof typeof USER_AVATAR_SIZES;

/** アバターに要る利用者の情報。名は `name`（表示名）か `username` のどちらでもよい。 */
export interface UserAvatarUser {
  id?: string;
  name?: string | null;
  username?: string | null;
  avatar_url?: string | null;
}
