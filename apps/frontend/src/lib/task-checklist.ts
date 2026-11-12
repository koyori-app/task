export type ChecklistProgress = { done: number; total: number };

/** 印の直後が `[ ]` / `[x]` で、その後ろに空白（半角空白・タブ）と、空白でない字が続く */
const TASK_BODY = /^\[([ \txX])\][ \t]+[^ \t]/;
const MARKER = /^([-*+]|\d{1,9}[.)])([ \t]+|$)/;
const THEMATIC_BREAK = /^([-*_])(?:[ \t]*\1){2,}[ \t]*$/;
const FENCE_OPEN = /^(`{3,}|~{3,})(.*)$/;
const FENCE_CLOSE = /^(`{3,}|~{3,})[ \t]*$/;
const QUOTE_MARK = /^( {0,3})>[ \t]?/;
const HEADING = /^#{1,6}(?:[ \t]|$)/;
/** 構文上の字下げ・空行にあたる空白は半角空白とタブだけ（`trim` 系は全角空白や NBSP も除く） */
const LEADING_WS = /^[ \t]+/;
const BLANK = /^[ \t]*$/;
/** 1 行に並べて外すリスト項目の印の数の上限。越えた印は本文として扱う（長い入力で時間と項目が膨らむのを防ぐ） */
const MAX_MARKERS_PER_LINE = 100;

/** 引用の中・リスト項目の中で開いた、閉じるまで集計から除く塊（フェンスと HTML コメント） */
type Block =
  | { kind: 'fence'; char: string; length: number; q: number; col: number }
  | { kind: 'comment'; q: number; col: number };

/**
 * 開いているリスト項目。`col` はその項目の本文が始まる桁（引用の印を除いた後の桁）、
 * `indent` は印の桁。`q` はその項目を囲む引用の深さ
 */
type Item = { q: number; col: number; indent: number; ordered: boolean };

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

/** 桁 `start` に置いた印の直後の空白（半角空白・タブ）が占める桁数。字下げを文字列で作らず桁だけを進める */
function gapAfterMarker(space: string, start: number): number {
  let width = start;
  for (let i = 0; i < space.length; i += 1) {
    width += space[i] === '\t' ? 4 - (width % 4) : 1;
  }
  return width - start;
}

/**
 * 行頭の引用の印（`>`）を最大 `max` 個まで剥がす。剥がした数と、
 * 各印の手前の字下げの桁数（`leads[k]` は k 個目の印の手前）も返す
 */
function stripQuote(
  line: string,
  max = Infinity,
): { rest: string; depth: number; leads: number[] } {
  let rest = line;
  const leads: number[] = [];
  while (leads.length < max) {
    const m = QUOTE_MARK.exec(rest);
    if (!m) break;
    leads.push(m[1]!.length);
    rest = rest.slice(m[0].length);
  }
  return { rest, depth: leads.length, leads };
}

/**
 * 説明文（Markdown）の GFM タスクリスト（`- [ ]` / `- [x]`）を数える。1 件も無ければ null。
 *
 * 描画器が出すチェックボックスの数に合わせる行の走査で、構文解析器は使わない
 * （描画器をクライアントへ載せるとバンドルが大きく増えるため）。親も子も 1 件ずつ数え、
 * フェンス付きコードブロックと HTML コメントの中は数えない。フェンスとコメントは、
 * 開いた引用・リスト項目の終わりで閉じる（描画器が入れ物の終わりで閉じるのと同じ）。
 *
 * 許容する食い違い（件数が 1 つずれる程度）:
 * - コメント以外の HTML ブロックの中
 * - 字下げコードの直後に書いた 1 以外の番号の項目（`    code\n2. [x] a`。描画器は項目にしない）
 * - 印の後ろに本文が次の行から始まる形（`- [x]\n  本文`。描画器はチェックボックスにする）
 * - 1 行に 100 個を超えて並べた項目の印（101 個目以降は本文として扱い、チェックボックスにしない）
 * - 引用と入れ子の項目を重ねた形の字下げの細部、setext 見出しの下線など CommonMark の細部
 *
 * 正確にするには構文解析器が要る。
 */
export function countChecklist(description: string | null | undefined): ChecklistProgress | null {
  if (!description || !description.includes('[')) return null;

  let done = 0;
  let total = 0;
  let open: Block | null = null;
  const items: Item[] = [];
  /** 直前の行が段落の文字の行か。その深さの引用の中にある */
  let paragraph = false;
  let paragraphQ = 0;

  /** 同じ引用の深さで開いている、いちばん内側のリスト項目の本文の桁 */
  const containerCol = (q: number): number => {
    for (let i = items.length - 1; i >= 0; i -= 1) {
      if (items[i]!.q === q) return items[i]!.col;
    }
    return 0;
  };

  for (const raw of description.split(/\r?\n/)) {
    if (open) {
      const { rest, depth } = stripQuote(raw, open.q);
      const blank = BLANK.test(rest);
      // 引用の印が足りない行は引用の外。字下げが足りない空でない行は項目の外
      if (depth === open.q && (blank || indentWidth(rest) >= open.col)) {
        if (open.kind === 'fence') {
          const close = FENCE_CLOSE.exec(rest.replace(LEADING_WS, ''));
          if (
            close &&
            indentWidth(rest) - open.col <= 3 &&
            close[1]![0] === open.char &&
            close[1]!.length >= open.length
          ) {
            open = null;
          }
        } else if (rest.includes('-->')) {
          open = null;
        }
        continue;
      }
      open = null;
    }

    const { rest, depth: q, leads } = stripQuote(raw);
    const indent = indentWidth(rest);
    const text0 = rest.replace(LEADING_WS, '');
    if (text0 === '') {
      // 空行は項目を閉じない。ただし引用の印だけの行は、そこから引用が始まるので外側の項目を閉じる
      if (q > 0) closeItems(items, q, leads, Infinity);
      paragraph = false;
      continue;
    }

    // 段落の続きの行（遅延継続行を含む）は入れ物を閉じない。印が付く行・ブロックの開始の行は続きではない
    if (paragraph && q <= paragraphQ) {
      const rel = indent - containerCol(q);
      if (rel >= 4 || !startsBlock(text0, q, paragraphQ, indent, items)) continue;
    }

    closeItems(items, q, leads, indent);

    const base = containerCol(q);
    // 入れ物の本文から 4 桁以上の字下げは字下げコード
    if (indent - base >= 4) {
      paragraph = false;
      continue;
    }

    let text = text0;
    if (THEMATIC_BREAK.test(text) || HEADING.test(text)) {
      paragraph = false;
      continue;
    }

    // リスト項目の印を前から外す（`- - [ ] a` や `1. - [x] a` のような同じ行の入れ子を含む）
    let col = indent;
    let marked = false;
    let code = false;
    const itemsBefore = items.length;
    for (;;) {
      const m = MARKER.exec(text);
      if (!m) break;
      // 上限までの印は項目として積み、越えた残りはその行の本文として扱う（後続行の所属判定を保つ）
      if (items.length - itemsBefore >= MAX_MARKERS_PER_LINE) break;
      const markerEnd = col + m[1]!.length;
      const empty = text.length === m[0].length;
      const gap = gapAfterMarker(m[2]!, markerEnd);
      // 印の後ろが 5 桁以上空くと、本文は字下げコード
      const codeAfter = !empty && gap >= 5;
      items.push({
        q,
        col: markerEnd + (empty || codeAfter ? 1 : gap),
        indent: col,
        ordered: /^\d/.test(m[1]!),
      });
      marked = true;
      if (codeAfter) {
        code = true;
        break;
      }
      col = items[items.length - 1]!.col;
      text = text.slice(m[0].length);
    }
    if (code) {
      paragraph = false;
      continue;
    }

    const contentCol = marked ? col : base;
    paragraph = false;
    paragraphQ = q;

    if (marked) {
      const task = TASK_BODY.exec(text);
      if (task) {
        total += 1;
        if (task[1] !== ' ' && task[1] !== '\t') done += 1;
        paragraph = true;
        continue;
      }
    }

    const fence = FENCE_OPEN.exec(text);
    if (fence) {
      const mark = fence[1]!;
      if (mark[0] !== '`' || !fence[2]!.includes('`')) {
        open = { kind: 'fence', char: mark[0]!, length: mark.length, q, col: contentCol };
        continue;
      }
    }

    if (text.startsWith('<!--')) {
      if (!text.includes('-->', 4)) open = { kind: 'comment', q, col: contentCol };
      continue;
    }

    paragraph = !BLANK.test(text);
  }

  return total > 0 ? { done, total } : null;
}

/**
 * 行が属さない項目を閉じる。引用の深さが浅い項目と、字下げが本文の桁に届かない項目が対象。
 * 引用の中の行は、外側の項目の本文の桁に届かないときも外側の項目を閉じる
 */
function closeItems(items: Item[], q: number, leads: number[], indent: number): void {
  while (items.length > 0) {
    const top = items[items.length - 1]!;
    const lead = top.q < q ? leads[top.q]! : indent;
    if (top.q > q || (top.q <= q && lead < top.col)) items.pop();
    else break;
  }
}

/** この行が段落を割って新しいブロック（項目・フェンス・コメント・見出し・区切り線）を始めるか */
function startsBlock(
  text: string,
  q: number,
  paragraphQ: number,
  indent: number,
  items: Item[],
): boolean {
  if (THEMATIC_BREAK.test(text) || HEADING.test(text) || text.startsWith('<!--')) return true;
  const fence = FENCE_OPEN.exec(text);
  if (fence && (fence[1]![0] !== '`' || !fence[2]!.includes('`'))) return true;
  const m = MARKER.exec(text);
  if (!m) return false;
  // 1 以外から始まる番号は、段落の続きの行（今の入れ子の中の行）では項目にならない
  if (/^\d/.test(m[1]!) && Number.parseInt(m[1]!, 10) !== 1) {
    return q < paragraphQ || items.some((item) => item.q === q && indent < item.col);
  }
  return true;
}
