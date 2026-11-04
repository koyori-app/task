/**
 * リアクションの型と表示の決まり。
 * 型は backend の契約の写し。段階 2 で openapi の生成型に置き換える。
 */
export type ReactionUser = { id: string; name: string; avatar_url: string | null };
/** `users` は先頭 5 人。全員は `ReactionUsersResponse` で別に取る */
export type ReactionSummary = {
  emoji: string;
  count: number;
  reacted: boolean;
  users: ReactionUser[];
};
export type ReactionUsersResponse = { emoji: string; count: number; users: ReactionUser[] };

export type FetchReactionUsers = (emoji: string) => Promise<ReactionUser[]>;

/** カードに並べる人数の上限（`ReactionSummary.users` の長さと同じ） */
export const REACTION_CARD_USER_LIMIT = 5;

/** 札の件数の字。1,000 以上は札の幅を保つため `999+` で止める（読み上げは実数） */
export function formatReactionCount(count: number): string {
  return count >= 1000 ? '999+' : String(count);
}

/** 札の読み上げ。`name` は日本語名か、未読み込みなら絵文字そのもの */
export function reactionChipLabel(name: string, count: number, reacted: boolean): string {
  return `${name} ${count} 件${reacted ? '、押しています' : ''}`;
}
