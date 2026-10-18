/**
 * コメントのメンション（`<@利用者ID>` の印）を扱う純関数。DOM にも CodeMirror にも依らぬ。
 *
 * 本文には利用者の id を印の形で持ち、名前は持たない。画面は印を `@今の名` の札に描く。
 * 入力欄でも印の文字は見せず（CommentEditor が札に置き換える）、札は一塊として扱う。
 * 印の形は backend の `payload::mentions`（`<@` + ハイフン付き UUID + `>`）と揃える。
 */

import { rankUsersByQuery } from '@/lib/user-search';

/** API が返す、印の id から引ける表示情報（`components['schemas']['MentionedUser']` と同じ形）。 */
export interface MentionUser {
  /**
   * member=いま入れる者 / outside=このプロジェクトに入れぬ者（そのテナントに属しておった者だけ名前が返る）/
   * gone=利用者の行が無い者
   */
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

/**
 * 札の字。id は出さない。名が引ける者は、いま入れる者も外れた者（プロジェクトの外・テナントを
 * 抜けたが、そのテナントに属しておった者）も `@名` で出す（灰色にするのは見た目の側）。
 * 名が引けぬ者（別のテナントの利用者・行の無い者）は定型の句。
 */
export function mentionLabel(user: MentionUser | undefined): string {
  if (!user) return '@（不明な利用者）';
  if (user.status !== 'gone' && user.name) return `@${user.name}`;
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

/**
 * 候補の絞り込み。前方一致を先に、次に部分一致。利用者の選択（UserSelector）と同じ規則
 * （`rankUsersByQuery`: 大文字小文字・全角半角・片仮名平仮名を問わぬ）で、上限だけを足す。
 */
export function filterCandidates(
  candidates: readonly MentionCandidate[],
  query: string,
  limit = 20,
): MentionCandidate[] {
  return rankUsersByQuery(candidates, query).slice(0, limit);
}

/**
 * クリップボードの形式（仕様: docs/features/tasks/4.collaboration-clipboard.md）。
 *
 * コピー・切り取りでは `text/plain` に `@今の名` を、`text/html` に札の要素を載せる。
 * 札の要素は `<span data-task-mention="1" data-user-id="…">@名</span>`。task の外のアプリは
 * `text/html` を読めば `@名` の字として貼れ、task の入力欄は印を見つけて `<@id>` に戻す。
 * 外から来た HTML に紛れた id を札にしてはならぬので、戻すのは版の印が既知の `1` で、
 * id が印の形（ハイフン付き UUID）の時だけとする。
 */
export const CLIPBOARD_MENTION_ATTR = 'data-task-mention';
export const CLIPBOARD_MENTION_ID_ATTR = 'data-user-id';
/** 知っておる版。これ以外の値は札として読まず、素の文字に倒す。 */
export const CLIPBOARD_MENTION_VERSION = '1';

const UUID_EXACT = new RegExp(`^${UUID_PATTERN}$`);

function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

/** 印を含む文を、札の要素を持つ HTML 断片にする。改行は `<br>`。 */
export function toClipboardHtml(
  text: string,
  resolve: (id: string) => MentionUser | undefined,
): string {
  const parts: string[] = [];
  let last = 0;
  const chunk = (s: string) => escapeHtml(s).replaceAll('\n', '<br>');
  for (const marker of findMarkers(text)) {
    parts.push(chunk(text.slice(last, marker.from)));
    const label = mentionLabel(resolve(marker.id));
    parts.push(
      `<span ${CLIPBOARD_MENTION_ATTR}="${CLIPBOARD_MENTION_VERSION}" ${CLIPBOARD_MENTION_ID_ATTR}="${marker.id}">${escapeHtml(label)}</span>`,
    );
    last = marker.to;
  }
  parts.push(chunk(text.slice(last)));
  return `<div>${parts.join('')}</div>`;
}

/**
 * `text/html` から、札の要素を `<@id>` に戻した文を取り出す。
 * 既知の版の札が一つも無ければ `null`（呼び手は素の貼り付けに任せる）。
 * 札の外の要素は字だけ拾い、`<br>` と段落の境は改行にする。
 */
export function fromClipboardHtml(html: string): string | null {
  if (!html.includes(CLIPBOARD_MENTION_ATTR)) return null;
  const doc = new DOMParser().parseFromString(html, 'text/html');
  let found = false;
  let out = '';
  const BLOCK = new Set(['DIV', 'P', 'LI', 'PRE', 'BLOCKQUOTE']);
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      out += (node.textContent ?? '').replace(/\r?\n/g, ' ');
      return;
    }
    if (!(node instanceof HTMLElement)) return;
    if (node.hasAttribute(CLIPBOARD_MENTION_ATTR)) {
      const id = node.getAttribute(CLIPBOARD_MENTION_ID_ATTR) ?? '';
      if (
        node.getAttribute(CLIPBOARD_MENTION_ATTR) === CLIPBOARD_MENTION_VERSION &&
        UUID_EXACT.test(id)
      ) {
        found = true;
        out += markerOf(id.toLowerCase());
      } else {
        out += node.textContent ?? '';
      }
      return;
    }
    if (node.tagName === 'BR') {
      out += '\n';
      return;
    }
    if (['SCRIPT', 'STYLE'].includes(node.tagName)) return;
    const block = BLOCK.has(node.tagName);
    if (block && out !== '' && !out.endsWith('\n')) out += '\n';
    node.childNodes.forEach(walk);
    if (block && !out.endsWith('\n')) out += '\n';
  };
  doc.body.childNodes.forEach(walk);
  if (!found) return null;
  return out.replace(/\n$/, '');
}
