/**
 * コメントのメンション（`<@利用者ID>` の印）を扱う純関数。DOM にも CodeMirror にも依らぬ。
 *
 * 本文には利用者の id を印の形で持ち、名前は持たない。画面は印を `@今の名` の札に描く。
 * 入力欄でも印の文字は見せず（CommentEditor が札に置き換える）、札は一塊として扱う。
 * 印の形は backend の `payload::mentions`（`<@` + ハイフン付き UUID + `>`）と揃える。
 */

/** API が返す、印の id から引ける表示情報（`components['schemas']['MentionedUser']` と同じ形）。 */
export interface MentionUser {
  /** member=いま入れる者 / outside=このプロジェクトに入れぬ者（名前は返らぬ）/ gone=退会 */
  status: 'member' | 'outside' | 'gone';
  name?: string | null;
  avatar_url?: string | null;
}

/** 印の id → 表示情報。API の `mentions` と同じ形。 */
export type MentionTable = Record<string, MentionUser>;

/** 補完の候補（`assignable-users` が返す利用者）。 */
export interface MentionCandidate {
  id: string;
  username: string;
  avatar_url?: string | null;
}

const UUID_PATTERN = '[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}';

/** 印の正規表現（毎回新しく作る。`g` の lastIndex を共有しないため）。 */
export function markerPattern(): RegExp {
  return new RegExp(`<@(${UUID_PATTERN})>`, 'g');
}

/** 利用者 id の印。 */
export function markerOf(id: string): string {
  return `<@${id}>`;
}

/** 本文中の印の位置（文字列のインデックス。UTF-16）と id。 */
export interface MarkerRange {
  from: number;
  to: number;
  id: string;
}

export function findMarkers(text: string): MarkerRange[] {
  const out: MarkerRange[] = [];
  for (const match of text.matchAll(markerPattern())) {
    const from = match.index ?? 0;
    out.push({ from, to: from + match[0].length, id: match[1].toLowerCase() });
  }
  return out;
}

/** 表に無い・名の無い者の札の字。id は出さない。 */
export function mentionLabel(user: MentionUser | undefined): string {
  if (!user) return '@（不明な利用者）';
  if (user.status === 'member' && user.name) return `@${user.name}`;
  if (user.status === 'gone') return '@（退会した利用者）';
  return '@（このプロジェクトの外の利用者）';
}

/** 表示の分割: 地の文と札。 */
export type BodySegment =
  | { kind: 'text'; text: string }
  | { kind: 'mention'; id: string; user: MentionUser | undefined };

export function splitBody(body: string, table: MentionTable): BodySegment[] {
  const out: BodySegment[] = [];
  let last = 0;
  for (const marker of findMarkers(body)) {
    if (marker.from > last) out.push({ kind: 'text', text: body.slice(last, marker.from) });
    out.push({ kind: 'mention', id: marker.id, user: lookup(table, marker.id) });
    last = marker.to;
  }
  if (last < body.length) out.push({ kind: 'text', text: body.slice(last) });
  return out;
}

/** 表は小文字の id で引く（backend は小文字で返す）。 */
export function lookup(table: MentionTable, id: string): MentionUser | undefined {
  return table[id] ?? table[id.toLowerCase()];
}

/** 印を札の字（`@名`）に直した、画面の外へ出す用の文。コピーで id を出さぬために使う。 */
export function toPlainText(
  text: string,
  resolve: (id: string) => MentionUser | undefined,
): string {
  return text.replace(markerPattern(), (_all, id: string) =>
    mentionLabel(resolve(id.toLowerCase())),
  );
}

/** 入力中の `@検索語`。`from` は `@` の位置、`query` は `@` の後ろから caret まで。 */
export interface MentionTrigger {
  from: number;
  query: string;
}

/** `@` の直前に置いてよい字（行頭・空白・開き括弧の類）。英数字の直後（`bob@example`）は除く。 */
const BEFORE_AT = /[\s　([{（「『"'“‘]/;

/**
 * caret より前の、同じ行の文字列から、補完を出すべき `@検索語` を探す。
 * 検索語に空白・`@`・`<`・`>` は含まぬ。印の中（`<@` の `@`）は `<` が直前に来るので拾わぬ。
 */
export function mentionTrigger(before: string): MentionTrigger | null {
  const match = /@([^\s@<>]*)$/.exec(before);
  if (!match) return null;
  const from = match.index;
  if (from > 0 && !BEFORE_AT.test(before[from - 1])) return null;
  return { from, query: match[1] };
}

/** 候補の絞り込み。前方一致を先に、次に部分一致。大文字小文字は無視。 */
export function filterCandidates(
  candidates: readonly MentionCandidate[],
  query: string,
  limit = 20,
): MentionCandidate[] {
  const q = query.toLowerCase();
  if (q === '') return candidates.slice(0, limit);
  const prefix: MentionCandidate[] = [];
  const partial: MentionCandidate[] = [];
  for (const candidate of candidates) {
    const name = candidate.username.toLowerCase();
    if (name.startsWith(q)) prefix.push(candidate);
    else if (name.includes(q)) partial.push(candidate);
  }
  return [...prefix, ...partial].slice(0, limit);
}
