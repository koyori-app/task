import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, userEvent, waitFor } from 'storybook/test';
import { ref } from 'vue';

import CommentBody from '@/components/tasks/CommentBody.vue';
import UserAvatar from '@/components/UserAvatar.vue';
import CommentEditor from '@/components/tasks/CommentEditor.vue';
import type { MentionTable } from '@/lib/comment-mention';
import { renderDescription } from '@/lib/markup-renderer';
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import '@/lib/remark-gfm/style.css';
import '@/lib/remark-kfm-mention/style.css';

/**
 * メンションの札（アバターと `@今の名` の丸い札）を、コメント・説明文（KFM）・入力欄（CodeMirror）の
 * 三つの描き手で並べる。三つは同じ一枚の CSS（`remark-kfm-mention/style.css`）を使うので、
 * 同じ状態の札は同じ見た目になる。play で次を固定する:
 * - 札に頭のアバター（画像か頭文字）と名、名の引けぬ者は定型の句だけ
 * - 灰色の札（外れた者・退会した者・名の引けぬ者）の字と地の対比が 4.5:1 以上で、縁が破線
 * - 札を含む行の高さが、札の無い行と同じ（札で行が広がらぬ）
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
 * 半透明の地（強調色を薄く敷いた札）は、明るい theme の画面の地（白）の上に重ねた色で比べる。
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

/** 三つの描き手に共通の確かめ。`root` の下の札と、札の有る行・無い行の高さを見る。 */
async function expectChips(
  root: HTMLElement,
  lines: { withChip: HTMLElement; without: HTMLElement },
): Promise<void> {
  const chips = [...root.querySelectorAll<HTMLElement>('.kfm-mention')];
  await expect(chips.map((chip) => chip.textContent)).toEqual([
    '@taro',
    '@aki',
    '@hanako',
    '@（このプロジェクトの外の利用者）',
    '@（不明な利用者）',
  ]);
  const [taro, aki, hanako, stranger, unknown] = chips;
  // 画像のある者は img、無い者は頭文字の丸。定型の句の札はアバター無し
  await expect(taro.querySelector('.kfm-mention-avatar img')).not.toBeNull();
  await expect(aki.querySelector('.kfm-mention-avatar')?.getAttribute('data-initials')).toBe('AK');
  await expect(hanako.querySelector('.kfm-mention-avatar img')).not.toBeNull();
  await expect(stranger.querySelector('.kfm-mention-avatar')).toBeNull();
  await expect(unknown.querySelector('.kfm-mention-avatar')).toBeNull();
  // 外れた者のアバターは灰色
  await expect(getComputedStyle(hanako.querySelector('.kfm-mention-avatar')!).filter).toContain(
    'grayscale',
  );
  // 灰色の札は対比 4.5:1 以上、縁は破線（色の薄さだけで分けぬ）
  for (const chip of [hanako, stranger, unknown]) {
    const style = getComputedStyle(chip);
    await expect(contrast(style.color, style.backgroundColor)).toBeGreaterThanOrEqual(4.5);
    await expect(style.borderTopStyle).toBe('dashed');
  }
  // いま入れる者の札も字が読める
  for (const chip of [taro, aki]) {
    const style = getComputedStyle(chip);
    await expect(contrast(style.color, style.backgroundColor)).toBeGreaterThanOrEqual(4.5);
    await expect(style.borderTopStyle).not.toBe('dashed');
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
          canvasElement.querySelectorAll('[data-testid="body"] .kfm-mention'),
        ).toHaveLength(5);
      },
      { timeout: 10_000 },
    );
    const pick = (id: string) =>
      canvasElement.querySelector<HTMLElement>(`[data-testid="${id}"] .kfm-content p`)!;
    await expectChips(pick('body'), { withChip: pick('with'), without: pick('without') });
    // 本人へのメンションは強調（反転の地）
    const self = pick('body').querySelector<HTMLElement>('.kfm-mention-self');
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
    await expectChips(pick('body'), { withChip: pick('with'), without: pick('without') });
    await expect(pick('body').querySelector('.kfm-mention-self')?.textContent).toBe('@aki');
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
    await expectChips(lines[0], { withChip: lines[1], without: lines[2] });
  },
};

/**
 * 札のアバターと、本物の UserAvatar の xs（頭文字・灰色）を並べ、描かれた値が同じことを縛る。
 * 札は CSS の写しなので、UserAvatar の段・頭文字の組・灰色のどれかが替われば、ここが落ちる。
 */
export const AvatarMatchesUserAvatar: Story = {
  render: () => ({
    components: { CommentBody, UserAvatar },
    setup: () => ({ ME, HANAKO, mentions }),
    template: `
      <div class="flex items-center gap-4 text-sm">
        <div data-testid="chip-active"><CommentBody comment-id="parity-active" :body="'<@' + ME + '>'" :mentions="mentions" /></div>
        <UserAvatar data-testid="avatar-active" size="xs" :user="{ name: 'aki' }" decorative />
        <div data-testid="chip-inactive"><CommentBody comment-id="parity-inactive" :body="'<@' + HANAKO + '>'" :mentions="{ [HANAKO]: { status: 'outside', name: 'hanako', avatar_url: null } }" /></div>
        <UserAvatar data-testid="avatar-inactive" size="xs" :user="{ name: 'hanako' }" decorative inactive />
      </div>`,
  }),
  play: async ({ canvasElement }) => {
    await waitFor(
      async () =>
        expect(
          canvasElement.querySelectorAll('[data-testid^="chip-"] .kfm-mention-avatar'),
        ).toHaveLength(2),
      { timeout: 10_000 },
    );
    const chipAvatar = (id: string) =>
      canvasElement.querySelector<HTMLElement>(`[data-testid="${id}"] .kfm-mention-avatar`)!;
    const userAvatar = (id: string) =>
      canvasElement.querySelector<HTMLElement>(`[data-testid="${id}"]`)!;
    for (const state of ['active', 'inactive']) {
      const chip = chipAvatar(`chip-${state}`);
      const root = userAvatar(`avatar-${state}`);
      const fallback = await waitFor(async () => {
        const el = root.querySelector<HTMLElement>('[data-slot="avatar-fallback"]');
        await expect(el).not.toBeNull();
        return el!;
      });
      const c = getComputedStyle(chip);
      const r = getComputedStyle(root);
      const f = getComputedStyle(fallback);
      // 丸の大きさ・頭文字の字の大きさ・地と字の色
      await expect(c.width).toBe(r.width);
      await expect(c.height).toBe(r.height);
      await expect(getComputedStyle(chip, '::before').fontSize).toBe(f.fontSize);
      await expect(contrast(c.backgroundColor, f.backgroundColor)).toBe(1);
      await expect(contrast(c.color, f.color)).toBe(1);
      // 灰色（不透明度と grayscale）
      await expect(c.opacity).toBe(r.opacity);
      await expect(c.filter).toBe(r.filter);
      // 頭文字の字は同じ
      await expect(chip.dataset.initials).toBe(fallback.textContent?.trim());
    }
    await expect(getComputedStyle(chipAvatar('chip-inactive')).filter).toContain('grayscale');
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
        const found = [...canvasElement.querySelectorAll<HTMLAnchorElement>('a.kfm-mention')];
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
    await expect(canvasElement.querySelector('span.kfm-mention-outside')?.textContent).toBe(
      '@hanako',
    );
    // 入力欄の札は link にせぬ
    await expect(canvasElement.querySelector('.cm-content a')).toBeNull();
    await expect(canvasElement.querySelector('.cm-content .kfm-mention')).not.toBeNull();

    // キーボードで最初の札へ focus し、輪が見え、Enter で飛ぶ
    (document.activeElement as HTMLElement | null)?.blur();
    await userEvent.tab();
    await expect(document.activeElement).toBe(links[0]);
    const style = getComputedStyle(links[0]);
    await expect(style.outlineStyle).toBe('solid');
    await expect(Number.parseFloat(style.outlineWidth)).toBeGreaterThanOrEqual(2);
    // 輪の色は地（白）に対して 3:1 以上
    await expect(contrast(style.outlineColor, '#fff')).toBeGreaterThanOrEqual(3);
    await userEvent.keyboard('{Enter}');
    await waitFor(async () =>
      expect(canvasElement.querySelector('[data-testid="clicked"]')?.textContent).toBe(
        `/acme/users/${TARO}`,
      ),
    );
  },
};
