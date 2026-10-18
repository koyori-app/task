/**
 * remark-kfm-mention — 本文中のメンションの印（`<@利用者ID>`）を札（`@今の名`）にする。
 *
 * 規則（kfm-mention テストで固定）:
 * - 印は `<@` + ハイフン付き UUID + `>`（コメントの入力欄・backend の `payload::mentions` と同じ形）。
 *   壊れた印（字数違い・閉じ無し）は素のテキストのまま
 * - text ノードだけを分割する。code / inlineCode / link / linkReference / definition / html の
 *   内側は走査しない（コードブロックとインラインコードの中の `<@id>` は札にしない）
 * - 札の字は `mentionLabel`（コメントの札と同じ）。id は HTML に出さない
 * - 状態は class（`kfm-mention-member` / `-outside` / `-gone` / `-unknown`）で運ぶ。見た目は
 *   `style.css`（コメントの札 `CommentMention.vue` と同じ色）
 *
 * 表（id → 状態・名前）はプラグインの options ではなく vfile の `data.kfm.mentions` から読む
 * （processor は profile ごとに memoize されるため、`remark-kfm-task-ref` と同じ理由）。
 * 表が渡されぬ描画（`mentions` を知らぬ消費側）は何もしない: 印は素の文字のまま残り、
 * 他の markdown の描き方は変わらない。表が空でも渡されれば、印は「不明な利用者」の札になる。
 *
 * 出力は mdast ノード ＋ data.hName / hProperties の型付き emit のみ（生 html・inline style なし）。
 */
import type { Parent, PhrasingContent, Root, Text } from 'mdast';
import { SKIP, visit } from 'unist-util-visit';

import { findMarkers, lookup, mentionLabel, type MentionTable } from '@/lib/comment-mention';

const SKIP_TYPES = new Set(['code', 'inlineCode', 'link', 'linkReference', 'definition', 'html']);

/** vfile の data.kfm に載せる描画時コンテキスト（_renderer.ts が process 時に渡す） */
export type KfmMentionData = { readonly mentions?: MentionTable };

/** 札の class。状態は既知の 4 つに限る（表の値をそのまま class にしない） */
const STATUS_CLASS = {
  member: 'kfm-mention-member',
  outside: 'kfm-mention-outside',
  gone: 'kfm-mention-gone',
} as const;

function readTable(data: Readonly<Record<string, unknown>>): MentionTable | undefined {
  const table = (data.kfm as KfmMentionData | undefined)?.mentions;
  return table !== null && typeof table === 'object' ? table : undefined;
}

function splitText(node: Text, table: MentionTable): PhrasingContent[] | undefined {
  const markers = findMarkers(node.value);
  if (markers.length === 0) return undefined;
  const parts: PhrasingContent[] = [];
  let last = 0;
  for (const marker of markers) {
    if (marker.from > last)
      parts.push({ type: 'text', value: node.value.slice(last, marker.from) });
    const user = lookup(table, marker.id);
    const statusClass =
      user && Object.hasOwn(STATUS_CLASS, user.status)
        ? STATUS_CLASS[user.status]
        : 'kfm-mention-unknown';
    parts.push({
      // span を出すための入れ物（hName で置き換わる）。意味を持つ mdast ノードではない
      type: 'emphasis',
      data: { hName: 'span', hProperties: { className: ['kfm-mention', statusClass] } },
      children: [{ type: 'text', value: mentionLabel(user) }],
    });
    last = marker.to;
  }
  if (last < node.value.length) parts.push({ type: 'text', value: node.value.slice(last) });
  return parts;
}

export function remarkKfmMention() {
  return (tree: Root, file: { readonly data: Readonly<Record<string, unknown>> }): void => {
    const table = readTable(file.data);
    if (table === undefined) return;
    visit(tree, (node, index, parent) => {
      if (SKIP_TYPES.has(node.type)) return SKIP;
      if (node.type !== 'text' || parent === undefined || index === undefined) return;
      const parts = splitText(node, table);
      if (parts === undefined) return;
      (parent as Parent).children.splice(index, 1, ...parts);
      // 差し込んだ札の内側へ降りない
      return [SKIP, index + parts.length];
    });
  };
}

/** 本プラグインが emit する class トークン（完全一致 allowlist）。`span` は DOMPurify 既定で通る */
export const kfmMentionSanitizeSchema = {
  classTokens: [
    'kfm-mention',
    'kfm-mention-member',
    'kfm-mention-outside',
    'kfm-mention-gone',
    'kfm-mention-unknown',
  ],
} as const;
