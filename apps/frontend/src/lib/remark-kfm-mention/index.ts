/**
 * remark-kfm-mention — 本文中のメンションの印（`<@利用者ID>`）を札（`@今の名`）にする。
 *
 * 規則（kfm-mention テストで固定）:
 * - 印は `<@` + ハイフン付き UUID + `>`（コメントの入力欄・backend の `payload::mentions` と同じ形）。
 *   壊れた印（字数違い・閉じ無し）は素のテキストのまま
 * - text ノードは札へ分割する。code / inlineCode と link / linkReference の内側は札にせず
 *   `@今の名` の字へ戻す。画像の alt、リンク・画像・定義の title、code の meta も同じ
 * - URL と生の HTML は意味を変えない
 * - 札の字は `mentionLabel`（コメントの札と同じ）。札の字・コピー・読み上げに id は出さない。
 *   プロフィールのカードを開く札（いま入れる者と、名の引ける外れた者）の根にだけ `data-user-id` を付ける
 * - いま入れる者で名が引ける札は、`data.kfm.mentionTenant` があればプロフィールへの link
 *   （`<a class="koyori-mention koyori-mention-member" href="/{tenant}/users/{利用者ID}">`）にする。道は表に在る id とテナントだけで組む。
 *   外れた者・退会した者・名の引けぬ者の札は link にせぬ
 * - 札は `span.koyori-mention-label`（`@名`）を 1 つだけ持ち、アバターは出さない（入力欄の札と同じ形）
 * - 状態は koyori の固定 class（`mentionClassNames`）で運ぶ。ログイン中の member と同じ id なら
 *   `koyori-mention-self` も付く。見た目は `@koyori-app/ui-vue/style.css`（`tailwind.css` が全体で読む）
 *
 * 表（id → 状態・名前）はプラグインの options ではなく vfile の `data.kfm.mentions` から読む
 * （processor は profile ごとに memoize されるため、`remark-kfm-task-ref` と同じ理由）。
 * 表が渡されぬ描画（`mentions` を知らぬ消費側）は何もしない: 印は素の文字のまま残り、
 * 他の markdown の描き方は変わらない。表が空でも渡されれば、印は「不明な利用者」の札になる。
 *
 * 出力は mdast ノード ＋ data.hName / hProperties の型付き emit のみ（生 html・inline style なし）。
 */
import type { Nodes, Parent, PhrasingContent, Root, Text } from 'mdast';
import { MENTION_CLASS_NAMES, mentionClassNames } from '@koyori-app/ui-vue';
import { SKIP, visit } from 'unist-util-visit';

import { mentionCardUserId } from '@/lib/mention-card';
import {
  findMarkers,
  lookup,
  mentionChip,
  toPlainText,
  type MentionTable,
} from '@/lib/comment-mention';

/** vfile の data.kfm に載せる描画時コンテキスト（_renderer.ts が process 時に渡す） */
export type KfmMentionData = {
  readonly mentions?: MentionTable;
  readonly currentUserId?: string | null;
  /** 札をプロフィールへの link にする時のテナント（表示用 id）。無い・字種外なら link にせぬ */
  readonly mentionTenant?: string;
};

/** テナントの表示用 id の字種（`remark-kfm-task-ref` と同じ）。URL の道に入るので fail-closed で見る */
const TENANT_RE = /^[a-z0-9][a-z0-9-]{0,62}$/i;
/** 印の id の形（`findMarkers` が返す小文字のハイフン付き UUID）。link の道に入れる前にもう一度見る */
const USER_ID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

function readTenant(data: KfmMentionData | undefined): string | undefined {
  const tenant = data?.mentionTenant;
  return typeof tenant === 'string' && TENANT_RE.test(tenant) ? tenant : undefined;
}

/**
 * 札の link の先（プロフィールの頁。`lib/task-display.ts` の `userProfileHref` と同じ道）。
 * いま入れる者で名が引ける札だけ。道は表に在る利用者の id とテナントだけから組み、本文の字は入れぬ。
 */
function profileHref(
  tenant: string | undefined,
  id: string,
  chip: ReturnType<typeof mentionChip>,
): string | undefined {
  if (!tenant || chip.state !== 'member' || !chip.named || !USER_ID_RE.test(id)) return undefined;
  return `/${tenant}/users/${id}`;
}

function readTable(data: Readonly<Record<string, unknown>>): MentionTable | undefined {
  const table = (data.kfm as KfmMentionData | undefined)?.mentions;
  return table !== null && typeof table === 'object' ? table : undefined;
}

function plainText(value: string, table: MentionTable): string {
  return toPlainText(value, (id) => lookup(table, id));
}

/**
 * 札を置けない文脈の字を、入れ子も含めて `@今の名` へ戻す。
 * URL・識別子・生 HTML は Markdown の意味を変えぬよう触らない。
 */
function replacePlainMentions(node: Nodes, table: MentionTable): void {
  switch (node.type) {
    case 'text':
    case 'inlineCode':
      node.value = plainText(node.value, table);
      return;
    case 'code':
      node.value = plainText(node.value, table);
      if (node.meta) node.meta = plainText(node.meta, table);
      return;
    case 'image':
      if (node.alt) node.alt = plainText(node.alt, table);
      if (node.title) node.title = plainText(node.title, table);
      return;
    case 'imageReference':
      if (node.alt) node.alt = plainText(node.alt, table);
      return;
    case 'link':
      if (node.title) node.title = plainText(node.title, table);
      for (const child of node.children) replacePlainMentions(child, table);
      return;
    case 'linkReference':
      for (const child of node.children) replacePlainMentions(child, table);
      return;
    case 'definition':
      if (node.title) node.title = plainText(node.title, table);
      return;
    default:
      if ('children' in node) {
        for (const child of node.children) replacePlainMentions(child, table);
      }
  }
}

function splitText(
  node: Text,
  table: MentionTable,
  currentUserId?: string | null,
  tenant?: string,
): PhrasingContent[] | undefined {
  const markers = findMarkers(node.value);
  if (markers.length === 0) return undefined;
  const parts: PhrasingContent[] = [];
  let last = 0;
  for (const marker of markers) {
    if (marker.from > last)
      parts.push({ type: 'text', value: node.value.slice(last, marker.from) });
    const user = lookup(table, marker.id);
    const chip = mentionChip(user, marker.id, currentUserId);
    // 状態の class は koyori が決める（知らない値は unknown、self は member の時だけ。表の値をそのまま class にしない）
    const className = mentionClassNames(chip.state, chip.self);
    const children: PhrasingContent[] = [
      {
        // 札の唯一の子。字を span で包む（koyori の固定 class の形）
        type: 'emphasis',
        data: { hName: 'span', hProperties: { className: ['koyori-mention-label'] } },
        children: [{ type: 'text', value: chip.label }],
      },
    ];
    const cardUserId = mentionCardUserId(user, marker.id);
    const hProperties = cardUserId ? { className, dataUserId: cardUserId } : { className };
    const href = profileHref(tenant, marker.id, chip);
    parts.push(
      href
        ? // プロフィールへの link（mdast の link。href は上で組んだ道だけ）
          { type: 'link', url: href, data: { hProperties }, children }
        : {
            // span を出すための入れ物（hName で置き換わる）。意味を持つ mdast ノードではない
            type: 'emphasis',
            data: { hName: 'span', hProperties },
            children,
          },
    );
    last = marker.to;
  }
  if (last < node.value.length) parts.push({ type: 'text', value: node.value.slice(last) });
  return parts;
}

export function remarkKfmMention() {
  return (tree: Root, file: { readonly data: Readonly<Record<string, unknown>> }): void => {
    const data = file.data.kfm as KfmMentionData | undefined;
    const table = readTable(file.data);
    if (table === undefined) return;
    visit(tree, (node, index, parent) => {
      if (
        node.type === 'code' ||
        node.type === 'inlineCode' ||
        node.type === 'link' ||
        node.type === 'linkReference' ||
        node.type === 'image' ||
        node.type === 'imageReference' ||
        node.type === 'definition'
      ) {
        replacePlainMentions(node, table);
        return SKIP;
      }
      if (node.type === 'html') return SKIP;
      if (node.type !== 'text' || parent === undefined || index === undefined) return;
      const parts = splitText(node, table, data?.currentUserId, readTenant(data));
      if (parts === undefined) return;
      (parent as Parent).children.splice(index, 1, ...parts);
      // 差し込んだ札の内側へ降りない
      return [SKIP, index + parts.length];
    });
  };
}

/**
 * 本プラグインが emit する class トークン（完全一致 allowlist）。koyori の固定 class の全集合をそのまま許す。
 * `span`・`a` と `href` は DOMPurify の既定で通る。
 * `data-user-id` も DOMPurify の既定（`ALLOW_DATA_ATTR`）で通る（生の HTML は字に escape されるので、本文からは書けない）
 */
export const kfmMentionSanitizeSchema = { classTokens: MENTION_CLASS_NAMES } as const;
