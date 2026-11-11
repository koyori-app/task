import { Mention } from '@koyori-app/ui-vue';
import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { ref } from 'vue';

import CommentBody from '@/components/tasks/CommentBody.vue';
import CommentEditor from '@/components/tasks/CommentEditor.vue';
import type { MentionTable } from '@/lib/comment-mention';
import { renderDescription } from '@/lib/markup-renderer';
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import '@/lib/remark-gfm/style.css';

/**
 * メンションの札（`@今の名`）を、コメント・説明文（KFM）・入力欄（CodeMirror）の三つの描き手で並べる。
 * 三つは koyori の固定 class（`@koyori-app/ui-vue/style.css`）で描くので、同じ状態の札は同じ見た目になる。
 * 札は `@名` の色の字と薄い地で、アバターは出さない。play で次を固定する:
 * - 札にアバターが無く、字は `koyori-mention-label` の span に入る。名の引けぬ者は定型の句だけ
 * - いま入れる者（本人を含む）の地は不透明で、字との対比が 4.5:1 以上。本人の強調はコメント・説明文だけ
 * - 灰色の札（外れた者・退会した者・名の引けぬ者）は地なしで、破線の下線。字・下線の対比が地に対して足りる
 * - 札を含む行の高さが、札の無い行と同じ（札で行が広がらぬ）
 * - link の札が、層の外の `.kfm-content a`（青い字・下線）に負けない
 * - task が組む札の HTML が、koyori の `<Mention>` と同じ形・見た目になる
 * a11y の検査は todo にせず落とす。
 */
const ME = '11111111-1111-4111-8111-111111111111';
const TARO = '22222222-2222-4222-8222-222222222222';
const HANAKO = '33333333-3333-4333-8333-333333333333';
const STRANGER = '44444444-4444-4444-8444-444444444444';
const UNKNOWN = '55555555-5555-4555-8555-555555555555';

/** 外へ出ぬ 1px の PNG（画像のある札を見せる）。 */
const IMAGE =
  'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

const mentions: MentionTable = {
  [ME]: { status: 'member', name: 'aki', avatar_url: null },
  [TARO]: { status: 'member', name: 'taro', avatar_url: IMAGE },
  [HANAKO]: { status: 'outside', name: 'hanako', avatar_url: IMAGE },
  [STRANGER]: { status: 'outside', name: null, avatar_url: null },
};

const BODY = `<@${TARO}> さん、確認をお願いします。<@${ME}> も見ます。前回は <@${HANAKO}> が担当でした。<@${STRANGER}> <@${UNKNOWN}>`;

const meta = {
  title: 'Tasks/MentionChip',
  parameters: { a11y: { test: 'error' } },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 色を 1px の canvas に塗って sRGB の値を読む（oklch 等の表記のままでも比べられる）。
 * 半透明の色は白に重ねた色で測る（不透明な色には効かぬ）。
 */
function toRgb(color: string): [number, number, number] {
  const canvas = document.createElement('canvas');
  canvas.width = 1;
  canvas.height = 1;
  const ctx = canvas.getContext('2d')!;
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, 1, 1);
  ctx.fillStyle = color;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  return [r, g, b];
}

function luminance([r, g, b]: [number, number, number]): number {
  const lin = (c: number) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

function contrast(fg: string, bg: string): number {
  const a = luminance(toRgb(fg));
  const b = luminance(toRgb(bg));
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/**
 * 札の後ろの地の色。自分から祖先へたどり、最初に不透明な `backgroundColor` を持つ要素の色を返す。
 * 見つからなければ body の色。地の無い札（灰色の 3 状態）の対比は、この色に対して測る。
 */
function backdrop(el: Element): string {
  for (let node: Element | null = el.parentElement; node; node = node.parentElement) {
    const color = getComputedStyle(node).backgroundColor;
    const [, alpha] = /^rgba\(.*,\s*([\d.]+)\)$/.exec(color) ?? [];
    if (color !== 'transparent' && (alpha === undefined || Number(alpha) === 1)) return color;
  }
  return getComputedStyle(document.body).backgroundColor;
}

/**
 * 三つの描き手に共通の確かめ。`root` の下の札と、札の有る行・無い行の高さを見る。
 * 本人の強調（`self`）はコメント・説明文だけで、入力欄は付けぬ。
 */
async function expectChips(
  root: HTMLElement,
  lines: { withChip: HTMLElement; without: HTMLElement },
  { self }: { self: boolean },
): Promise<void> {
  const chips = [...root.querySelectorAll<HTMLElement>('.koyori-mention')];
  const labels = [
    '@taro',
    '@aki',
    '@hanako',
    '@（このプロジェクトの外の利用者）',
    '@（不明な利用者）',
  ];
  await expect(chips.map((chip) => chip.textContent)).toEqual(labels);
  // アバターは出さず、子は字の span 1 つだけ
  for (const [i, chip] of chips.entries()) {
    await expect(chip.querySelector('img, [data-initials]')).toBeNull();
    await expect(chip.children).toHaveLength(1);
    await expect(chip.children[0].classList.contains('koyori-mention-label')).toBe(true);
    await expect(chip.children[0].textContent).toBe(labels[i]);
  }
  const [taro, aki, hanako, stranger, unknown] = chips;
  // いま入れる者の札: 地は不透明、字との対比が 4.5:1 以上、下線は無い
  for (const chip of [taro, aki]) {
    const style = getComputedStyle(chip);
    await expect(style.backgroundColor).toMatch(/^rgb\(\d+, \d+, \d+\)$/);
    await expect(contrast(style.color, style.backgroundColor)).toBeGreaterThanOrEqual(4.5);
    await expect(style.textDecorationLine).toBe('none');
  }
  // 本人の強調
  const own = getComputedStyle(aki);
  const other = getComputedStyle(taro);
  if (self) {
    await expect(aki.classList.contains('koyori-mention-self')).toBe(true);
    await expect(own.fontWeight).toBe('600');
    await expect(own.color).not.toBe(other.color);
  } else {
    await expect(aki.classList.contains('koyori-mention-self')).toBe(false);
    await expect(own.color).toBe(other.color);
    await expect(own.backgroundColor).toBe(other.backgroundColor);
  }
  // 灰色の札: 地なし・破線の下線（色の薄さだけで分けぬ）。字は 4.5:1、下線は 3:1 以上
  for (const chip of [hanako, stranger, unknown]) {
    const style = getComputedStyle(chip);
    const ground = backdrop(chip);
    await expect(style.backgroundColor).toBe('rgba(0, 0, 0, 0)');
    await expect(style.textDecorationLine).toBe('underline');
    await expect(style.textDecorationStyle).toBe('dashed');
    await expect(contrast(style.color, ground)).toBeGreaterThanOrEqual(4.5);
    await expect(contrast(style.textDecorationColor, ground)).toBeGreaterThanOrEqual(3);
  }
  // 札で行が広がらぬ
  await expect(lines.withChip.getBoundingClientRect().height).toBe(
    lines.without.getBoundingClientRect().height,
  );
}

export const Comment: Story = {
  render: () => ({
    components: { CommentBody },
    setup: () => ({ BODY, ME, TARO, mentions }),
    template: `
      <div style="width: 1100px" class="space-y-2">
        <div data-testid="body"><CommentBody comment-id="chip-body" :body="BODY" :mentions="mentions" :current-user-id="ME" /></div>
        <div data-testid="with"><CommentBody comment-id="chip-with" :body="'行の高さ <@' + TARO + '> を見る'" :mentions="mentions" /></div>
        <div data-testid="without"><CommentBody comment-id="chip-without" body="行の高さ を見る" :mentions="mentions" /></div>
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    // コメントの本文は KFM の描き手で非同期に描かれるので、三つとも描き終えるのを待つ
    await waitFor(
      async () => {
        await expect(canvasElement.querySelectorAll('[data-testid] .kfm-content p')).toHaveLength(
          3,
        );
        await expect(
          canvasElement.querySelectorAll('[data-testid="body"] .koyori-mention'),
        ).toHaveLength(5);
      },
      { timeout: 10_000 },
    );
    const pick = (id: string) =>
      canvasElement.querySelector<HTMLElement>(`[data-testid="${id}"] .kfm-content p`)!;
    await expectChips(
      pick('body'),
      { withChip: pick('with'), without: pick('without') },
      { self: true },
    );
    // 本人へのメンションは強調
    const self = pick('body').querySelector<HTMLElement>('.koyori-mention-self');
    await expect(self?.textContent).toBe('@aki');
  },
};

export const Description: Story = {
  loaders: [
    async () => ({
      html: {
        body: await renderDescription(BODY, { mentions, currentUserId: ME }),
        with: await renderDescription(`行の高さ <@${TARO}> を見る`, { mentions }),
        without: await renderDescription('行の高さ を見る', { mentions }),
      },
    }),
  ],
  render: (_args, { loaded }) => ({
    setup: () => ({ html: loaded.html, KFM_CONTENT_CLASS }),
    template: `
      <div style="width: 1100px" class="space-y-2 text-sm">
        <div data-testid="body" :class="KFM_CONTENT_CLASS" v-html="html.body" />
        <div data-testid="with" :class="KFM_CONTENT_CLASS" v-html="html.with" />
        <div data-testid="without" :class="KFM_CONTENT_CLASS" v-html="html.without" />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const pick = (id: string) =>
      canvasElement.querySelector<HTMLElement>(`[data-testid="${id}"] p`)!;
    await expectChips(
      pick('body'),
      { withChip: pick('with'), without: pick('without') },
      { self: true },
    );
    await expect(pick('body').querySelector('.koyori-mention-self')?.textContent).toBe('@aki');
  },
};

export const Editor: Story = {
  render: () => ({
    components: { CommentEditor },
    setup: () => ({
      value: ref(`${BODY}\n行の高さ <@${TARO}> を見る\n行の高さ を見る`),
      mentions,
    }),
    template: `
      <div style="width: 1100px">
        <CommentEditor v-model="value" :mentions="mentions" aria-label="コメントを入力" />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const lines = await waitFor(async () => {
      const found = [...canvasElement.querySelectorAll<HTMLElement>('.cm-line')];
      await expect(found).toHaveLength(3);
      await expect(found[0].querySelectorAll('.cm-mention')).toHaveLength(5);
      return found;
    });
    await expectChips(lines[0], { withChip: lines[1], without: lines[2] }, { self: false });
  },
};

/**
 * task が組む札の HTML（CommentBody）と、koyori の `<Mention>` を並べ、形と描かれた値が同じことを縛る。
 * task の札は koyori の固定 class の HTML を手で組む（v-html・CodeMirror は Vue の部品を置けぬ）ので、
 * 要素・class・子の形、色・地・太さ・下線・余白・角丸・行の高さのどれかが食い違えば、ここが落ちる。
 * 札は flex の子に直に置かず、`data-case` を付けた入れ物で包む（koyori docs の注意）。
 */
export const MatchesKoyoriMention: Story = {
  render: () => ({
    components: { CommentBody, Mention },
    setup: () => ({ ME, TARO, HANAKO, STRANGER, UNKNOWN, mentions }),
    template: `
      <div class="grid grid-cols-2 gap-4 text-sm" style="width: 800px">
        <div data-side="task">
          <div data-case="member"><CommentBody comment-id="m-member" :body="'<@' + TARO + '>'" :mentions="mentions" mention-tenant="acme" :current-user-id="ME" /></div>
          <div data-case="self"><CommentBody comment-id="m-self" :body="'<@' + ME + '>'" :mentions="mentions" mention-tenant="acme" :current-user-id="ME" /></div>
          <div data-case="outside"><CommentBody comment-id="m-outside" :body="'<@' + HANAKO + '>'" :mentions="mentions" mention-tenant="acme" :current-user-id="ME" /></div>
          <div data-case="gone"><CommentBody comment-id="m-gone" :body="'<@' + STRANGER + '>'" :mentions="{ [STRANGER]: { status: 'gone', name: null } }" mention-tenant="acme" :current-user-id="ME" /></div>
          <div data-case="unknown"><CommentBody comment-id="m-unknown" :body="'<@' + UNKNOWN + '>'" :mentions="mentions" mention-tenant="acme" :current-user-id="ME" /></div>
        </div>
        <div data-side="koyori">
          <div data-case="member"><Mention label="@taro" status="member" :href="'/acme/users/' + TARO" /></div>
          <div data-case="self"><Mention label="@aki" status="member" self :href="'/acme/users/' + ME" /></div>
          <div data-case="outside"><Mention label="@hanako" status="outside" /></div>
          <div data-case="gone"><Mention label="@（退会した利用者）" status="gone" /></div>
          <div data-case="unknown"><Mention label="@（不明な利用者）" status="unknown" /></div>
        </div>
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    await waitFor(
      async () =>
        expect(canvasElement.querySelectorAll('[data-side="task"] .koyori-mention')).toHaveLength(
          5,
        ),
      { timeout: 10_000 },
    );
    const pick = (side: string, name: string) =>
      canvasElement.querySelector<HTMLElement>(
        `[data-side="${side}"] [data-case="${name}"] .koyori-mention`,
      )!;
    for (const name of ['member', 'self', 'outside', 'gone', 'unknown']) {
      const mine = pick('task', name);
      const theirs = pick('koyori', name);
      await expect(mine.tagName, name).toBe(theirs.tagName);
      await expect([...mine.classList], name).toEqual([...theirs.classList]);
      await expect(mine.children, name).toHaveLength(1);
      await expect(theirs.children, name).toHaveLength(1);
      await expect(mine.children[0].tagName, name).toBe(theirs.children[0].tagName);
      await expect(mine.children[0].className, name).toBe(theirs.children[0].className);
      await expect(mine.textContent, name).toBe(theirs.textContent);
      const a = getComputedStyle(mine);
      const b = getComputedStyle(theirs);
      for (const prop of [
        'color',
        'backgroundColor',
        'fontWeight',
        'textDecorationLine',
        'textDecorationStyle',
        'textDecorationColor',
        'paddingLeft',
        'borderTopLeftRadius',
        'lineHeight',
      ] as const) {
        await expect(a[prop], `${name}: ${prop}`).toBe(b[prop]);
      }
    }
  },
};

/**
 * 札のプロフィールへの link（説明文の KFM とコメント）。キーボードで focus でき、focus の輪が見え、Enter で
 * link の先へ飛ぶこと（story の中では飛ばずに、click で受けた先を記す）。入力欄の札は link にせぬ。
 */
export const ProfileLinks: Story = {
  loaders: [
    async () => ({
      description: await renderDescription(`説明 <@${TARO}> と <@${HANAKO}>`, {
        mentions,
        mentionTenant: 'acme',
      }),
    }),
  ],
  render: (_args, { loaded }) => ({
    components: { CommentBody, CommentEditor },
    setup: () => {
      const clicked = ref<string[]>([]);
      // 飛んだ先を記し、story の iframe は動かさぬ
      const onClick = (event: MouseEvent) => {
        const link = (event.target as HTMLElement | null)?.closest('a');
        if (!link) return;
        event.preventDefault();
        clicked.value.push(link.getAttribute('href') ?? '');
      };
      return {
        html: loaded.description,
        KFM_CONTENT_CLASS,
        mentions,
        clicked,
        onClick,
        TARO,
        ME,
        value: ref(`入力欄 <@${TARO}>`),
      };
    },
    template: `
      <div style="width: 900px" class="space-y-3 text-sm" @click.capture="onClick">
        <div data-testid="description" :class="KFM_CONTENT_CLASS" v-html="html" />
        <div data-testid="comment"><CommentBody comment-id="links" :body="'コメント <@' + TARO + '> と <@' + ME + '>'" :mentions="mentions" :current-user-id="ME" mention-tenant="acme" /></div>
        <CommentEditor v-model="value" :mentions="mentions" aria-label="コメントを入力" />
        <pre data-testid="clicked">{{ clicked.join(' ') }}</pre>
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    const links = await waitFor(
      async () => {
        const found = [...canvasElement.querySelectorAll<HTMLAnchorElement>('a.koyori-mention')];
        // 説明文の taro・コメントの taro と aki（外れた hanako は link にせぬ）
        await expect(found).toHaveLength(3);
        return found;
      },
      { timeout: 10_000 },
    );
    await expect(links.map((a) => a.getAttribute('href'))).toEqual([
      `/acme/users/${TARO}`,
      `/acme/users/${TARO}`,
      `/acme/users/${ME}`,
    ]);
    await expect(canvasElement.querySelector('span.koyori-mention-outside')?.textContent).toBe(
      '@hanako',
    );
    // 入力欄の札は link にせぬ
    await expect(canvasElement.querySelector('.cm-content a')).toBeNull();
    await expect(canvasElement.querySelector('.cm-content .koyori-mention')).not.toBeNull();

    // 層の外の `.kfm-content a`（青い字・下線）に負けず、入力欄の member の札と同じ見た目（focus の前に測る）
    const link = getComputedStyle(links[0]);
    const editorChip = getComputedStyle(
      canvasElement.querySelector<HTMLElement>('.cm-content .koyori-mention-member')!,
    );
    await expect(link.textDecorationLine).toBe('none');
    await expect(link.color).toBe(editorChip.color);
    await expect(link.backgroundColor).toBe(editorChip.backgroundColor);

    // キーボードで最初の札へ focus し、輪が見え、Enter で飛ぶ
    (document.activeElement as HTMLElement | null)?.blur();
    await userEvent.tab();
    await expect(document.activeElement).toBe(links[0]);
    const style = getComputedStyle(links[0]);
    await expect(style.outlineStyle).toBe('solid');
    await expect(Number.parseFloat(style.outlineWidth)).toBeGreaterThanOrEqual(2);
    // 輪の色は札の後ろの地に対して 3:1 以上
    await expect(contrast(style.outlineColor, backdrop(links[0]))).toBeGreaterThanOrEqual(3);
    await userEvent.keyboard('{Enter}');
    await waitFor(async () =>
      expect(canvasElement.querySelector('[data-testid="clicked"]')?.textContent).toBe(
        `/acme/users/${TARO}`,
      ),
    );
  },
};
