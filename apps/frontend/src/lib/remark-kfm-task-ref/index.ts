/**
 * remark-kfm-task-ref — 本文中のタスク参照 (`TASK-58` 等) をタスク詳細へのリンクにする。
 *
 * 規則 (kfm-task-ref テストで固定):
 * - マッチは `([A-Z][A-Z0-9]{1,9})-(\d+)`。前が英数字・`_`・`/`・`-`、後が英数字・`_`・`-`
 *   なら不一致 (`foo/TASK-58` や `TASK-58-foo` を拾わない)。全角括弧・句読点は許す
 * - text ノードだけを分割する。code / inlineCode / link / linkReference / definition /
 *   html の内側は走査しない (autolink 済みの URL や明示リンクを二重にリンク化しない)
 * - 存在確認はしない (描画時に API を叩かない)。出力は
 *   `<a class="kfm-task-ref" href="/{tenant}/projects/{KEY}/tasks/{KEY-N}">`
 *
 * テナントはプラグインの options ではなく vfile の `data.kfm.taskRef.tenant` から読む。
 * processor は profile ごとに memoize されるため (_renderer.ts の getProcessor)、
 * テナントごとに processor を作ると singleton にテナント数ぶん溜まってメモリが漏れる。
 * tenant が無い・字種外なら何もしない (URL に入る値なので fail-closed。throw はしない)。
 *
 * 出力は mdast の link ノード ＋ data.hProperties の型付き emit のみ (生 html・inline style なし)。
 */
import type { Link, Root, Text } from 'mdast';
import { SKIP, visit } from 'unist-util-visit';

const TASK_REF_RE = /(?<![\w/-])([A-Z][A-Z0-9]{1,9})-(\d+)(?![\w-])/g;
const TENANT_RE = /^[a-z0-9][a-z0-9-]{0,62}$/i;
const SKIP_TYPES = new Set(['code', 'inlineCode', 'link', 'linkReference', 'definition', 'html']);

/** vfile の data.kfm に載せる描画時コンテキスト (_renderer.ts が process 時に渡す) */
export type KfmTaskRefData = { readonly taskRef?: { readonly tenant: string } };

function readTenant(data: Readonly<Record<string, unknown>>): string | undefined {
  const tenant = (data.kfm as KfmTaskRefData | undefined)?.taskRef?.tenant;
  return typeof tenant === 'string' && TENANT_RE.test(tenant) ? tenant : undefined;
}

function splitText(node: Text, tenant: string): Array<Text | Link> | undefined {
  const parts: Array<Text | Link> = [];
  let last = 0;
  for (const match of node.value.matchAll(TASK_REF_RE)) {
    const [ref, key] = match;
    if (match.index > last) {
      parts.push({ type: 'text', value: node.value.slice(last, match.index) });
    }
    parts.push({
      type: 'link',
      url: `/${tenant}/projects/${key}/tasks/${ref}`,
      data: { hProperties: { className: ['kfm-task-ref'] } },
      children: [{ type: 'text', value: ref }],
    });
    last = match.index + ref.length;
  }
  if (parts.length === 0) return undefined;
  if (last < node.value.length) parts.push({ type: 'text', value: node.value.slice(last) });
  return parts;
}

export function remarkKfmTaskRef() {
  // 第 2 引数は unified が渡す VFile。data しか使わないため構造型で受ける
  // (vfile への直接依存を足さない)。
  return (tree: Root, file: { readonly data: Readonly<Record<string, unknown>> }): void => {
    const tenant = readTenant(file.data);
    if (tenant === undefined) return;
    visit(tree, (node, index, parent) => {
      if (SKIP_TYPES.has(node.type)) return SKIP;
      if (node.type !== 'text' || parent === undefined || index === undefined) return;
      const parts = splitText(node, tenant);
      if (parts === undefined) return;
      parent.children.splice(index, 1, ...(parts as typeof parent.children));
      // 差し込んだ link の内側へ降りない
      return [SKIP, index + parts.length];
    });
  };
}

/** 本プラグインが emit する class トークン (完全一致 allowlist)。相対 href は DOMPurify 既定で通る */
export const kfmTaskRefSanitizeSchema = {
  classTokens: ['kfm-task-ref'],
} as const;
