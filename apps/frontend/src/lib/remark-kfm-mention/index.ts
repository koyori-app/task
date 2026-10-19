/**
 * remark-kfm-mention — 本文中のメンションの印（`<@利用者ID>`）を札（`@今の名`）にする。
 *
 * 規則（kfm-mention テストで固定）:
 * - 印は `<@` + ハイフン付き UUID + `>`（コメントの入力欄・backend の `payload::mentions` と同じ形）。
 *   壊れた印（字数違い・閉じ無し）は素のテキストのまま
 * - text ノードだけを分割する。code / inlineCode / link / linkReference / definition / html の
 *   内側は走査しない（コードブロックとインラインコードの中の `<@id>` は札にしない）
 * - 札の字は `mentionLabel`（コメントの札と同じ）。id は HTML に出さない
 * - いま入れる者で名が引ける札は、`data.kfm.mentionTenant` があればプロフィールへの link
 *   （`<a class="kfm-mention …" href="/{tenant}/users/{利用者ID}">`）にする。道は表に在る id とテナントだけで組む。
 *   外れた者・退会した者・名の引けぬ者の札は link にせぬ
 * - 名で出る札は頭にアバター（`span.kfm-mention-avatar`。画像は `safeAvatarUrl` を通った URL だけ）を置く。
 *   定型の句の札にはアバターを出さない（`mentionChip` の判じ。入力欄・コメントの札と同じ）
 * - 状態は class（`kfm-mention-member` / `-outside` / `-gone` / `-unknown`）で運ぶ。ログイン中の
 *   member と同じ id なら `kfm-mention-self` も付ける。見た目は `style.css` に集約する
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

import { findMarkers, lookup, mentionChip, type MentionTable } from '@/lib/comment-mention';
import { avatarInitials } from '@/lib/initials';

const SKIP_TYPES = new Set(['code', 'inlineCode', 'link', 'linkReference', 'definition', 'html']);

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
  if (!tenant || chip.state !== 'member' || !chip.avatar || !USER_ID_RE.test(id)) return undefined;
  return `/${tenant}/users/${id}`;
}

/** 札の class。状態は既知の 4 つに限る（表の値をそのまま class にしない） */
const STATUS_CLASS = {
  member: 'kfm-mention-member',
  outside: 'kfm-mention-outside',
  gone: 'kfm-mention-gone',
} as const;

/**
 * 札の頭のアバター（`UserAvatar` の xs と同じ丸）。頭文字は字の node にせず `data-initials` に載せ、
 * CSS の `::before` で描く（コピーした文・読み上げに頭文字が混ざらぬ）。画像は安全な URL の時だけ
 * `<img alt="">` を重ね、読み込めねば下の頭文字が見える。丸ごと `aria-hidden`（名は隣の字が担う）。
 */
function avatarNode(name: string, src: string | null): PhrasingContent {
  return {
    type: 'emphasis',
    data: {
      hName: 'span',
      hProperties: {
        className: ['kfm-mention-avatar'],
        dataInitials: avatarInitials(name),
        ariaHidden: 'true',
      },
    },
    children: src
      ? [
          {
            type: 'emphasis',
            data: { hName: 'img', hProperties: { src, alt: '' } },
            children: [],
          },
        ]
      : [],
  };
}

function readTable(data: Readonly<Record<string, unknown>>): MentionTable | undefined {
  const table = (data.kfm as KfmMentionData | undefined)?.mentions;
  return table !== null && typeof table === 'object' ? table : undefined;
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
    const statusClass =
      user && Object.hasOwn(STATUS_CLASS, user.status)
        ? STATUS_CLASS[user.status]
        : 'kfm-mention-unknown';
    const className = ['kfm-mention', statusClass];
    if (user?.status === 'member' && currentUserId && marker.id === currentUserId.toLowerCase()) {
      className.push('kfm-mention-self');
    }
    const chip = mentionChip(user, marker.id, currentUserId);
    const children: PhrasingContent[] = [];
    if (chip.avatar) children.push(avatarNode(chip.avatar.name, chip.avatar.src));
    children.push({ type: 'text', value: chip.label });
    const href = profileHref(tenant, marker.id, chip);
    parts.push(
      href
        ? // プロフィールへの link（mdast の link。href は上で組んだ道だけ）
          { type: 'link', url: href, data: { hProperties: { className } }, children }
        : {
            // span を出すための入れ物（hName で置き換わる）。意味を持つ mdast ノードではない
            type: 'emphasis',
            data: { hName: 'span', hProperties: { className } },
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
      if (SKIP_TYPES.has(node.type)) return SKIP;
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
 * 本プラグインが emit する class トークン（完全一致 allowlist）。`span`・`a`・`img` と、その `href`・`src`・`alt` は
 * DOMPurify 既定で通る（札の link のために足した許可は無い）
 */
export const kfmMentionSanitizeSchema = {
  classTokens: [
    'kfm-mention',
    'kfm-mention-member',
    'kfm-mention-outside',
    'kfm-mention-gone',
    'kfm-mention-unknown',
    'kfm-mention-self',
    'kfm-mention-avatar',
  ],
} as const;
