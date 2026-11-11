import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import { expect, fireEvent, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';

import CommentBody from '@/components/tasks/CommentBody.vue';
import type { MentionTable } from '@/lib/comment-mention';
import '@/lib/remark-gfm/style.css';

/**
 * メンションの札のプロフィールカード（koyori の HoverCard）。コメント本文の札にポインターを載せると開く。
 * 名前・画像・本人かどうかは本文が持つ `mentions` の表から読み込みを待たずに出し、所属・自己紹介・
 * 「ゲスト」の印はプロフィール API が返ってから足す。play で次を固定する:
 * - メンバー: 所属・自己紹介（2 行で切れる）・プロフィールへのリンク。同じ人を開き直しても取りに行かぬ
 * - 本人は「あなた」、客分は「ゲスト」の印。外れた者は灰色のアバターと注記で、取れた後にリンクが出る
 * - 退会した者は固定の文言だけで取りに行かず、リンクも無い
 * - 読み込み中・取得失敗は名前と画像だけ。名の引けぬ札とタッチではカードを開かぬ
 * 明るい面は `data-theme="light"` の囲みで確かめる（カードの色は koyori のトークンで、囲みに追従する）。
 */
const TENANT = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const AKI = '11111111-1111-4111-8111-111111111111';
const TARO = '22222222-2222-4222-8222-222222222222';
const SATO = '33333333-3333-4333-8333-333333333333';
const HANAKO = '44444444-4444-4444-8444-444444444444';
const GONE = '55555555-5555-4555-8555-555555555555';
const STRANGER = '66666666-6666-4666-8666-666666666666';
const UNKNOWN = '77777777-7777-4777-8777-777777777777';

/** 外へ出ぬ画像（avatar_url に渡す。読み込みに成功して img が出る） */
const IMAGE = `data:image/svg+xml,${encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#f6a5c0"/><stop offset="1" stop-color="#8f7cf7"/></linearGradient></defs><rect width="80" height="80" fill="url(#g)"/></svg>',
)}`;

const mentions: MentionTable = {
  [AKI]: { status: 'member', name: 'aki', avatar_url: null },
  [TARO]: { status: 'member', name: 'taro', avatar_url: IMAGE },
  [SATO]: { status: 'member', name: 'sato', avatar_url: null },
  [HANAKO]: { status: 'outside', name: 'hanako', avatar_url: IMAGE },
  [GONE]: { status: 'gone', name: null },
  [STRANGER]: { status: 'outside', name: null, avatar_url: null },
};

const BODY = `<@${TARO}> さん、確認をお願いします。<@${AKI}> も見ます。<@${SATO}> さんにも共有。前回は <@${HANAKO}> が担当でした。<@${GONE}> の作業は引き継ぎ済み。<@${STRANGER}> <@${UNKNOWN}>`;

const BIO =
  '画面と部品のデザインを担当しています。Figma のファイルはプロジェクトの資料にあります。質問があれば気軽に声をかけてください。デザインの相談も歓迎です。';

const profiles: Record<string, Record<string, unknown>> = {
  [TARO]: {
    id: TARO,
    username: 'taro',
    account_kind: 'human',
    bio: BIO,
    avatar_url: IMAGE,
    department: 'プロダクト部',
    position: 'デザイナー',
    team: 'UI チーム',
    membership: 'Member',
  },
  [AKI]: {
    id: AKI,
    username: 'aki',
    account_kind: 'human',
    bio: null,
    avatar_url: null,
    department: '開発部',
    position: 'エンジニア',
    team: null,
    membership: 'Member',
  },
  [SATO]: {
    id: SATO,
    username: 'sato',
    account_kind: 'human',
    bio: null,
    avatar_url: null,
    department: null,
    position: null,
    team: null,
    membership: 'Guest',
  },
  [HANAKO]: {
    id: HANAKO,
    username: 'hanako',
    account_kind: 'human',
    bio: null,
    avatar_url: IMAGE,
    department: '開発部',
    position: 'エンジニア',
    team: null,
    membership: 'Member',
  },
};

type FetchMode = 'ok' | 'pending' | 'forbidden';

let fetchSpy: ReturnType<typeof fn>;

/** プロフィール API の応答を差し替える（それ以外の取得は元の fetch に通す）。後片付けで元の fetch に戻す。 */
function mockProfiles(mode: FetchMode) {
  return () => {
    const originalFetch = globalThis.fetch;
    fetchSpy = fn(async (request: RequestInfo | URL, init?: RequestInit) => {
      const target = new URL(
        typeof request === 'string' ? request : request instanceof URL ? request.href : request.url,
        'http://localhost',
      );
      // 本文の描画器が使う外部の取得（コードの着色の wasm など）は触らぬ
      if (!target.pathname.includes(`/tenants/${TENANT}/users/`))
        return originalFetch(request, init);
      if (mode === 'pending') return new Promise<Response>(() => {});
      const id = target.pathname.split('/').pop() ?? '';
      const json = (body: unknown, status = 200) =>
        new Response(JSON.stringify(body), {
          status,
          headers: { 'Content-Type': 'application/json' },
        });
      if (mode === 'forbidden') return json({ message: 'forbidden' }, 403);
      const profile = profiles[id];
      return profile ? json(profile) : json({ message: 'not found' }, 404);
    });
    globalThis.fetch = fetchSpy as unknown as typeof fetch;
    return () => {
      globalThis.fetch = originalFetch;
    };
  };
}

const profileCalls = (id?: string) =>
  fetchSpy.mock.calls.filter(([request]) =>
    new URL((request as Request).url, 'http://localhost').pathname.endsWith(
      `/tenants/${TENANT}/users/${id ?? ''}`,
    ),
  ).length;

/** プロフィール API を呼んだ回数（全員分） */
const anyProfileCalls = () =>
  fetchSpy.mock.calls.filter(([request]) =>
    new URL((request as Request).url, 'http://localhost').pathname.includes(
      `/tenants/${TENANT}/users/`,
    ),
  ).length;

const meta = {
  title: 'Tasks/MentionHoverCard',
  parameters: { a11y: { test: 'error' } },
  decorators: [
    () => ({
      setup() {
        provide(
          VUE_QUERY_CLIENT,
          new QueryClient({ defaultOptions: { queries: { retry: false } } }),
        );
      },
      template: '<story />',
    }),
  ],
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

/** 本文（札つき）。明るい面は囲みで切り替える（カードの DOM も囲みの中なのでトークンを継ぐ） */
function bodyStory(light: boolean) {
  const wrapperAttrs = light
    ? 'data-theme="light" style="background: var(--koyori-color-surface); color: var(--koyori-color-text); padding: 16px; min-height: 360px"'
    : 'style="padding: 16px; min-height: 360px"';
  return () => ({
    components: { CommentBody },
    setup: () => ({ BODY, mentions, AKI, TENANT }),
    template: `
      <div ${wrapperAttrs}>
        <CommentBody
          :body="BODY"
          comment-id="story"
          :mentions="mentions"
          :current-user-id="AKI"
          :tenant-id="TENANT"
          mention-tenant="acme"
        />
      </div>`,
  });
}

/** 札（`@名`）にポインターを載せ、名前つきの dialog が開くまで待つ。 */
async function openCard(canvasElement: HTMLElement, chipText: string, name: string) {
  const canvas = within(canvasElement);
  const label = await canvas.findByText(chipText, undefined, { timeout: 15_000 });
  const chip = label.closest('.koyori-mention') as HTMLElement;
  await userEvent.hover(chip);
  const dialog = await canvas.findByRole('dialog', { name });
  // 現れる動き（120ms）の間は透明なので、見えるまで待つ
  await waitFor(() => expect(dialog).toBeVisible());
  return { chip, dialog };
}

const hrefOf = (dialog: HTMLElement) =>
  within(dialog)
    .getByRole('link', { name: /プロフィールを見る/ })
    .getAttribute('href');

function memberPlay(countFetches: boolean): Story['play'] {
  return async ({ canvasElement }) => {
    const { chip, dialog } = await openCard(canvasElement, '@taro', 'taro のプロフィール');
    const card = within(dialog);
    await expect(card.getByText('taro')).toBeVisible();
    await expect(await card.findByText('デザイナー · プロダクト部 · UI チーム')).toBeVisible();
    const bio = dialog.querySelector('.bio') as HTMLElement;
    await expect(bio).toHaveTextContent('画面と部品のデザインを担当しています。');
    // 自己紹介は 2 行で切れる
    await expect(bio.scrollHeight).toBeGreaterThan(bio.clientHeight);
    await expect(hrefOf(dialog)).toBe(`/acme/users/${TARO}`);
    await expect(card.getByText('Esc で閉じる')).toBeVisible();
    await waitFor(() => expect(dialog.querySelector('img')).not.toBeNull());
    await expect(card.queryByText('あなた')).toBeNull();
    await expect(card.queryByText('ゲスト')).toBeNull();

    if (!countFetches) return;
    // 閉じて開き直しても、同じ人は取りに行かない
    await expect(profileCalls(TARO)).toBe(1);
    await userEvent.unhover(chip);
    await waitFor(() => expect(within(canvasElement).queryByRole('dialog')).toBeNull());
    await userEvent.hover(chip);
    const reopened = await within(canvasElement).findByRole('dialog', {
      name: 'taro のプロフィール',
    });
    await waitFor(() => expect(reopened.querySelector('.bio')).not.toBeNull());
    await expect(profileCalls(TARO)).toBe(1);
  };
}

export const Member: Story = {
  name: 'メンバー',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: memberPlay(true),
};

export const MemberLight: Story = {
  name: 'メンバー（明）',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(true),
  play: memberPlay(false),
};

function selfPlay(): Story['play'] {
  return async ({ canvasElement }) => {
    const { dialog } = await openCard(canvasElement, '@aki', 'aki のプロフィール');
    const card = within(dialog);
    await expect(card.getByText('あなた')).toBeVisible();
    await expect(await card.findByText('エンジニア · 開発部')).toBeVisible();
    await expect(card.queryByText('ゲスト')).toBeNull();
  };
}

export const Self: Story = {
  name: '本人',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: selfPlay(),
};

export const SelfLight: Story = {
  name: '本人（明）',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(true),
  play: selfPlay(),
};

function guestPlay(): Story['play'] {
  return async ({ canvasElement }) => {
    const { dialog } = await openCard(canvasElement, '@sato', 'sato のプロフィール');
    const card = within(dialog);
    await expect(await card.findByText('ゲスト')).toBeVisible();
    // 所属も自己紹介も無いので、その行ごと出ない
    await expect(dialog.querySelector('.meta')).toBeNull();
    await expect(dialog.querySelector('.body')).toBeNull();
    await expect(hrefOf(dialog)).toBe(`/acme/users/${SATO}`);
  };
}

export const Guest: Story = {
  name: 'ゲスト',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: guestPlay(),
};

export const GuestLight: Story = {
  name: 'ゲスト（明）',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(true),
  play: guestPlay(),
};

function outsidePlay(): Story['play'] {
  return async ({ canvasElement }) => {
    const { dialog } = await openCard(canvasElement, '@hanako', 'hanako のプロフィール');
    const card = within(dialog);
    await expect(
      card.getByText(
        'このプロジェクトのメンバーではありません。メンションしても通知は届きません。',
      ),
    ).toBeVisible();
    // アバターは灰色
    const avatar = dialog.querySelector('.is-inactive') as HTMLElement;
    await expect(getComputedStyle(avatar).filter).toContain('grayscale');
    // プロフィールが取れるまでリンクは出ず、取れた後に出る
    await expect(await card.findByRole('link', { name: /プロフィールを見る/ })).toBeVisible();
    await expect(hrefOf(dialog)).toBe(`/acme/users/${HANAKO}`);
    await expect(await card.findByText('エンジニア · 開発部')).toBeVisible();
  };
}

export const Outside: Story = {
  name: '外れた者',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: outsidePlay(),
};

export const OutsideLight: Story = {
  name: '外れた者（明）',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(true),
  play: outsidePlay(),
};

function gonePlay(): Story['play'] {
  return async ({ canvasElement }) => {
    const { dialog } = await openCard(canvasElement, '@（退会した利用者）', '退会した利用者');
    const card = within(dialog);
    await expect(card.getByText('退会した利用者')).toBeVisible();
    await expect(card.getByText('この利用者はテナントから退会しています。')).toBeVisible();
    await expect(card.queryByRole('link')).toBeNull();
    // 行が無いので取りに行かぬ
    await expect(anyProfileCalls()).toBe(0);
  };
}

export const Gone: Story = {
  name: '退会した者',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: gonePlay(),
};

export const GoneLight: Story = {
  name: '退会した者（明）',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(true),
  play: gonePlay(),
};

/** 取得が終わらぬ間・失敗した後は、表の値（名前と画像）とメンバーのリンクだけ。 */
function namesOnlyPlay(expectSettled: boolean): Story['play'] {
  return async ({ canvasElement }) => {
    const { dialog } = await openCard(canvasElement, '@taro', 'taro のプロフィール');
    await expect(within(dialog).getByText('taro')).toBeVisible();
    await waitFor(() => expect(dialog.querySelector('img')).not.toBeNull());
    await expect(hrefOf(dialog)).toBe(`/acme/users/${TARO}`);
    await waitFor(() => expect(profileCalls(TARO)).toBe(1));
    if (expectSettled) await new Promise((resolve) => setTimeout(resolve, 300));
    await expect(dialog.querySelector('.meta')).toBeNull();
    await expect(dialog.querySelector('.bio')).toBeNull();
    await expect(within(dialog).queryByText('ゲスト')).toBeNull();
  };
}

export const Loading: Story = {
  name: '読み込み中',
  beforeEach: mockProfiles('pending'),
  render: bodyStory(false),
  play: namesOnlyPlay(false),
};

export const LoadingLight: Story = {
  name: '読み込み中（明）',
  beforeEach: mockProfiles('pending'),
  render: bodyStory(true),
  play: namesOnlyPlay(false),
};

export const Unavailable: Story = {
  name: '取得失敗（403）',
  beforeEach: mockProfiles('forbidden'),
  render: bodyStory(false),
  play: namesOnlyPlay(true),
};

export const NoCard: Story = {
  name: 'カードを出さない札',
  beforeEach: mockProfiles('ok'),
  render: bodyStory(false),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const labelOf = async (text: string) =>
      (await canvas.findByText(text, undefined, { timeout: 15_000 })).closest(
        '.koyori-mention',
      ) as HTMLElement;
    const wait = () => new Promise((resolve) => setTimeout(resolve, 700));

    // 名の引けぬ外れた者・表に無い者
    for (const text of ['@（このプロジェクトの外の利用者）', '@（不明な利用者）']) {
      await userEvent.hover(await labelOf(text));
      await wait();
      await expect(canvas.queryByRole('dialog')).toBeNull();
      await userEvent.unhover(await labelOf(text));
    }

    // タッチでは開かない（タップはリンクの移動のまま）
    const taro = await labelOf('@taro');
    await fireEvent.pointerOver(taro, { pointerType: 'touch' });
    await wait();
    await expect(canvas.queryByRole('dialog')).toBeNull();
    await expect(anyProfileCalls()).toBe(0);
  },
};
