/**
 * タスクの説明文のチェックボックスを切り替えるための、原文と描画済み HTML の書き換え (TASK-539)。
 *
 * `offset` は描画器 (rehype-kfm-task-offset) が `data-kfm-task-offset` に出す値で、
 * 改行を LF に直した本文での `[` の添字。原文は CRLF のことがあるので、ここで原文の添字へ直す。
 * 描画器は import しない (クライアントへ描画器を載せない規約)。属性名だけ共有の定数を使う。
 */
import { KFM_TASK_OFFSET_ATTR } from '@/lib/rehype-kfm-task-offset/_attr';

/**
 * LF 換算の `offset` を原文の添字へ直す。`\r\n` と単独の `\r` を 1 字と数える
 * (描画器の `text.replace(/\r\n?/g, '\n')` と同じ)。範囲外なら null。
 */
function toSourceIndex(source: string, offset: number): number | null {
  if (!Number.isInteger(offset) || offset < 0) return null;
  let lf = 0;
  let i = 0;
  while (i < source.length) {
    if (lf === offset) return i;
    if (source[i] === '\r' && source[i + 1] === '\n') i += 2;
    else i += 1;
    lf += 1;
  }
  return lf === offset ? i : null;
}

/**
 * `offset` の `[` の中の 1 字を `checked` に合わせて `x` か半角空白に替えた本文を返す。
 * そこが `[`・（空白かタブか `x` か `X`）・`]` でなければ null。ほかの字は改行コードも含めて変えない。
 */
export function toggleChecklistAt(source: string, offset: number, checked: boolean): string | null {
  const open = toSourceIndex(source, offset);
  if (open === null || source[open] !== '[' || source[open + 2] !== ']') return null;
  if (!/[ \txX]/.test(source[open + 1] ?? '')) return null;
  return `${source.slice(0, open + 1)}${checked ? 'x' : ' '}${source.slice(open + 2)}`;
}

/**
 * `data-kfm-task-offset="<offset>"` を持つ `input` の `checked=""` を付け外しした HTML を返す。
 * 見つからなければ null。サーバが新しい本文を描いた HTML と同じ形にする
 * (描画器は `type="checkbox"` の直後に `checked=""` を置く)。
 */
export function setChecklistHtmlChecked(
  html: string,
  offset: number,
  checked: boolean,
): string | null {
  if (!Number.isInteger(offset) || offset < 0) return null;
  const needle = `${KFM_TASK_OFFSET_ATTR}="${offset}"`;
  let from = 0;
  for (;;) {
    const at = html.indexOf(needle, from);
    if (at < 0) return null;
    from = at + needle.length;
    const tagStart = html.lastIndexOf('<', at);
    const tagEnd = tagStart < 0 ? -1 : html.indexOf('>', tagStart);
    if (tagStart < 0 || tagEnd < 0) return null;
    // 本文の字に同じ並びがあっても、実際の `input` タグの内側のものだけを対象にする
    if (tagEnd < at) continue;
    const tag = html.slice(tagStart, tagEnd + 1);
    // 属性値の中の `"` と `<` `>` は escape されるので、`<input ` で始まるタグの内側だけを見る。
    if (!tag.startsWith('<input ') || !tag.includes(' type="checkbox"')) continue;
    const base = tag.replace(' checked=""', '');
    const next = checked ? base.replace(' type="checkbox"', ' type="checkbox" checked=""') : base;
    return html.slice(0, tagStart) + next + html.slice(tagEnd + 1);
  }
}
