export type ChecklistProgress = { done: number; total: number };

const TASK_ITEM = /^[ \t]*(?:[-*+]|\d{1,9}[.)])[ \t]+\[([ \txX])\][ \t]+\S/;
const LIST_ITEM = /^[ \t]*(?:[-*+]|\d{1,9}[.)])(?:[ \t]|$)/;
const FENCE_OPEN = /^[ \t]*(`{3,}|~{3,})(.*)$/;

/** タブを 4 桁の空白に直した字下げの桁数 */
function indentWidth(line: string): number {
  let width = 0;
  for (const ch of line) {
    if (ch === ' ') width += 1;
    else if (ch === '\t') width += 4 - (width % 4);
    else break;
  }
  return width;
}

/** 行頭の引用の印（`>`）を剥がす。引用の中のタスクリストも描画されるので数える */
function stripQuote(line: string): string {
  let rest = line;
  for (;;) {
    const next = rest.replace(/^ {0,3}>[ \t]?/, '');
    if (next === rest) return rest;
    rest = next;
  }
}

/**
 * 説明文（Markdown）の GFM タスクリスト（`- [ ]` / `- [x]`）を数える。1 件も無ければ null。
 *
 * 描画器が出すチェックボックスの数に合わせる行の走査で、構文解析器は使わない
 * （描画器をクライアントへ載せるとバンドルが大きく増えるため）。親も子も 1 件ずつ数え、
 * フェンス付きコードブロックと HTML コメントの中は数えない。
 *
 * 許容する食い違い（件数が 1 つずれる程度）: コメント以外の HTML ブロックの中、
 * リストの項目の中に字下げだけで書いたコードブロックの中、遅延継続行など CommonMark の細部。
 */
export function countChecklist(description: string | null | undefined): ChecklistProgress | null {
  if (!description || !description.includes('[')) return null;

  let done = 0;
  let total = 0;
  let fence: { char: string; length: number } | null = null;
  let inComment = false;
  let listOpen = false;

  for (const rawLine of description.split(/\r?\n/)) {
    const line = stripQuote(rawLine);

    if (fence) {
      const close = line.match(/^[ \t]*(`{3,}|~{3,})[ \t]*$/);
      if (close && close[1]![0] === fence.char && close[1]!.length >= fence.length) fence = null;
      continue;
    }

    if (inComment) {
      if (line.includes('-->')) inComment = false;
      continue;
    }

    const open = line.match(FENCE_OPEN);
    if (open) {
      const mark = open[1]!;
      if (mark[0] !== '`' || !open[2]!.includes('`')) {
        fence = { char: mark[0]!, length: mark.length };
        continue;
      }
    }

    if (indentWidth(line) <= 3 && /^[ \t]*<!--/.test(line)) {
      const start = line.indexOf('<!--');
      if (!line.includes('-->', start + 4)) inComment = true;
      continue;
    }

    if (line.trim() === '') continue;

    const task = line.match(TASK_ITEM);
    if (task && (indentWidth(line) < 4 || listOpen)) {
      total += 1;
      if (task[1] !== ' ' && task[1] !== '\t') done += 1;
    }

    if (LIST_ITEM.test(line) && (indentWidth(line) < 4 || listOpen)) {
      listOpen = true;
    } else if (indentWidth(line) === 0) {
      listOpen = false;
    }
  }

  return total > 0 ? { done, total } : null;
}
