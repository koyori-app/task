/**
 * メンションの札のプロフィールカード（HoverCard）の純関数。DOM の取得にも Vue にも依らぬ。
 *
 * 札の根の要素に付ける `data-user-id` で利用者を特定し、容れ物が持つ `mentions` の表から名前・画像・状態を引く。
 * 札を作る側（remark-kfm-mention）と、カードを開く側（MentionHoverCard）が同じ規則（`mentionCardUserId`）を
 * 通るので、セレクターに合う札は必ず対象が引ける（空のカードは開かぬ）。
 */
import { lookup, mentionChip, type MentionTable, type MentionUser } from '@/lib/comment-mention';

/**
 * カードを開く札。CommentBody / 説明の v-html の容れ物の中だけで使う
 * （容れ物で範囲を切るので、入力欄の CodeMirror の札には当たらない）。
 * 退会した者の札は id を持たず、class で起点にする（カードは固定の文言）。
 */
export const MENTION_CARD_SELECTOR = '.koyori-mention[data-user-id], .koyori-mention-gone';

/** 印の id の形（`findMarkers` が返す小文字のハイフン付き UUID） */
const USER_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * 札に `data-user-id` を付けるか。いま入れる者と、名の引ける外れた者の id（小文字の UUID）だけ。
 * 退会した者（行が無い）・名の無い外れた者（プロフィールは必ず 404）・不明な者には付けぬ。
 */
export function mentionCardUserId(user: MentionUser | undefined, id: string): string | null {
  if (!user || (user.status !== 'member' && user.status !== 'outside')) return null;
  if (!user.name) return null;
  const normalized = id.toLowerCase();
  return USER_ID_RE.test(normalized) ? normalized : null;
}

export type MentionCardTarget =
  | { kind: 'gone' }
  | {
      kind: 'member' | 'outside';
      userId: string;
      name: string;
      avatarUrl: string | null;
      self: boolean;
    };

/** 起点の要素からカードの対象を引く。開かぬ札（表に無い・表と食い違う・名の無い札）は null。 */
export function mentionCardTarget(
  trigger: Element,
  table: MentionTable,
  currentUserId?: string | null,
): MentionCardTarget | null {
  if (trigger.classList.contains('koyori-mention-gone')) return { kind: 'gone' };
  const id = (trigger.getAttribute('data-user-id') ?? '').toLowerCase();
  const user = lookup(table, id);
  const userId = mentionCardUserId(user, id);
  if (!user || userId === null) return null;
  return {
    kind: user.status as 'member' | 'outside',
    userId,
    name: user.name as string,
    avatarUrl: user.avatar_url || null,
    self: mentionChip(user, userId, currentUserId).self,
  };
}

/** 「役職 · 部署 · チーム」。値のある項目だけを ' · ' でつなぐ。全部無ければ null。 */
export function profileMetaLine(profile: {
  position?: string | null;
  department?: string | null;
  team?: string | null;
}): string | null {
  const parts = [profile.position, profile.department, profile.team]
    .map((value) => value?.trim() ?? '')
    .filter((value) => value !== '');
  return parts.length > 0 ? parts.join(' · ') : null;
}

/**
 * 本文の HTML にカードを開く札があるか（容れ物ごとの HoverCard を置くかの判断）。
 * 偽陽性（札の字に `koyori-mention` と書いた本文など）は、HoverCard が 1 つ余分に置かれるだけで害は無い。
 */
export function hasMentionCards(html: string): boolean {
  return html.includes('koyori-mention');
}
