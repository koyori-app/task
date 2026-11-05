import type { components } from '@/generated/api';

/** リアクションの型と表示の決まり */
export type ReactionUser = components['schemas']['CommentUser'];
/** `users` は先頭 5 人。全員は `ReactionUsersResponse` で別に取る */
export type ReactionSummary = components['schemas']['ReactionSummary'];
export type ReactionUsersResponse = components['schemas']['ReactionUsersResponse'];

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

/**
 * 押した時点の表示。再取得で正しい値に置き換わるまでの見込みなので、人の並びは先頭に足すだけにする。
 * 絵文字は完全一致で比べる（backend は異体字セレクタの有無が違えば別の絵文字として数える）。
 */
export function applyReaction(
  reactions: readonly ReactionSummary[],
  emoji: string,
  reacted: boolean,
  me: ReactionUser | null,
): ReactionSummary[] {
  const current = reactions.find((reaction) => reaction.emoji === emoji);
  if (Boolean(current?.reacted) === reacted) return [...reactions];
  if (!current) return [...reactions, { emoji, count: 1, reacted, users: me ? [me] : [] }];
  const others = current.users.filter((user) => user.id !== me?.id);
  const next: ReactionSummary = reacted
    ? {
        emoji,
        count: current.count + 1,
        reacted,
        users: (me ? [me, ...others] : others).slice(0, REACTION_CARD_USER_LIMIT),
      }
    : { emoji, count: current.count - 1, reacted, users: others };
  return next.count > 0
    ? reactions.map((reaction) => (reaction === current ? next : reaction))
    : reactions.filter((reaction) => reaction !== current);
}
