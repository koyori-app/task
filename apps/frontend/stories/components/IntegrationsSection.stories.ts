import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';

import IntegrationsSection from '@/components/projects/IntegrationsSection.vue';
import { forgetAuthorizeToken, forgetSelectToken } from '@/lib/github-select-token';

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';
const PROJECT_UUID = '00000000-0000-4000-8000-000000000010';

const connectedIntegration = {
  connected: true,
  repo_owner: 'koyori-app',
  repo_name: 'koyori',
  connected_at: '2026-07-01T09:00:00Z',
};

const notConnectedIntegration = {
  connected: false,
  repo_owner: null,
  repo_name: null,
  connected_at: null,
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

type MockOptions = {
  connected?: boolean;
  failIntegration?: boolean;
  /** GitHub のユーザー認可から戻った状態にする（認可トークンをフラグメントで渡す） */
  authorized?: boolean;
  /** POST /github/authorized-installations/select を、この本文の 403 で失敗させる */
  authorizedSelectForbidden?: string;
};

const AUTHORIZE_TOKEN = 'story-authorize-token';
const authorizedInstallations = [
  { candidate_id: '00000000-0000-4000-8000-0000000000b1', account_login: 'koyori-app' },
  { candidate_id: '00000000-0000-4000-8000-0000000000b2', account_login: 'acme-org' },
];

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch(options: MockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    let connected = options.connected ?? false;
    if (options.authorized) {
      // callback はトークンをフラグメントで返す。セクションがマウント時に退避して URL から落とす。
      window.history.replaceState(
        window.history.state,
        '',
        `#github_installations=${AUTHORIZE_TOKEN}`,
      );
    }
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      const pathname = new URL(url, 'http://localhost').pathname;
      if (method === 'POST' && pathname.endsWith('/github/import')) {
        return new Response(null, { status: 202 });
      }
      if (pathname.endsWith('/github/integration')) {
        if (method === 'DELETE') {
          connected = false;
          return new Response(null, { status: 204 });
        }
        if (options.failIntegration) return jsonResponse({ message: 'error' }, 500);
        return jsonResponse(connected ? connectedIntegration : notConnectedIntegration);
      }
      if (method === 'GET' && pathname.endsWith('/github/installations')) {
        return jsonResponse({ installations: [] });
      }
      if (method === 'GET' && pathname.endsWith('/github/authorized-installations')) {
        return jsonResponse({ installations: authorizedInstallations });
      }
      if (method === 'POST' && pathname.endsWith('/github/authorized-installations/select')) {
        if (options.authorizedSelectForbidden)
          return jsonResponse({ message: options.authorizedSelectForbidden }, 403);
        return jsonResponse({ select_token: 'story-select-token' });
      }
      if (method === 'GET' && pathname.endsWith('/github/repositories')) {
        return jsonResponse({
          repositories: [
            { owner: 'acme-org', name: 'backend' },
            { owner: 'acme-org', name: 'frontend' },
          ],
        });
      }
      return jsonResponse({ message: 'not-found' }, 404);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
      // トークンはタブ内に退避されるので、次のストーリーへ持ち越さない
      forgetAuthorizeToken(PROJECT_UUID);
      forgetSelectToken(PROJECT_UUID);
    };
  };
}

function storyDecorator() {
  return () => ({
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: {
          queries: { retry: false, gcTime: 0, staleTime: 0 },
          mutations: { retry: false },
        },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
    },
    template: '<story />',
  });
}

const meta = {
  title: 'Components/Projects/IntegrationsSection',
  component: IntegrationsSection,
  tags: ['autodocs'],
  parameters: {
    layout: 'padded',
    docs: {
      description: {
        component:
          'プロジェクト設定の連携セクション。GitHub 連携の状態表示・解除・Issue の取り込み開始を fetch モックで検証。' +
          'GitHub へ進むボタン（インストール・ユーザー認可）は外部 URL へ実遷移するため story では押さず、ユニットテストで担保。' +
          'Slack / Figma カードは API 実装後に追加予定。',
      },
    },
  },
  decorators: [storyDecorator()],
  args: {
    tenantId: TENANT_UUID,
    projectId: PROJECT_UUID,
  },
} satisfies Meta<typeof IntegrationsSection>;

export default meta;
type Story = StoryObj<typeof meta>;

export const NotConnected: Story = {
  name: '未連携（連携するボタン表示）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByRole('heading', { name: '連携' })).resolves.toBeInTheDocument();
    await expect(canvas.findByText('GitHub')).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText('コミットや Pull Request をタスクに紐付けます。'),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByRole('button', { name: '連携する' })).resolves.toBeInTheDocument();
    await expect(canvas.queryByText(/Slack|Figma/)).not.toBeInTheDocument();
  },
};

export const AccountCandidates: Story = {
  name: '候補のパネル（追加と、インストール済みの確認の 2 つの入口）',
  beforeEach: mockFetch(),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();

    await user.click(await canvas.findByRole('button', { name: '連携する' }));

    await expect(
      canvas.findByText('利用中の GitHub アカウント・組織はありません。'),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByText(/他のテナントで連携済みのアカウント・組織は、GitHub/),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('button', { name: '別の GitHub アカウント・組織を追加' }),
    ).resolves.toBeInTheDocument();
    await expect(
      canvas.findByRole('button', { name: 'インストール済みのアカウント・組織を GitHub で確認' }),
    ).resolves.toBeInTheDocument();
  },
};

export const AuthorizedInstallations: Story = {
  name: 'GitHub で確認したアカウント・組織（認可から戻った直後）',
  beforeEach: mockFetch({ authorized: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();

    await expect(
      canvas.findByText('GitHub で確認したアカウント・組織'),
    ).resolves.toBeInTheDocument();
    await expect(canvas.findByText('koyori-app')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('acme-org')).resolves.toBeInTheDocument();
    // トークンはヘッダーで送り、URL からは落とす
    const list = (fetchSpy!.mock.calls as [Request | string][])
      .map(([req]) => req)
      .filter((req): req is Request => typeof req !== 'string')
      .find((req) => req.url.endsWith('/github/authorized-installations'));
    await expect(list!.headers.get('X-Github-Authorize-Token')).toBe(AUTHORIZE_TOKEN);
    await expect(window.location.hash).toBe('');

    // 選ぶと、既存のリポジトリ選択へ進む
    await user.click(canvas.getAllByRole('button', { name: 'これを使う' })[1]!);
    await expect(canvas.findByText('連携するリポジトリを選択')).resolves.toBeInTheDocument();
    await expect(canvas.findByText('acme-org/backend')).resolves.toBeInTheDocument();
  },
};

export const AuthorizedAdminRequired: Story = {
  name: 'GitHub で確認したアカウント・組織（管理者権限が足りない）',
  beforeEach: mockFetch({
    authorized: true,
    authorizedSelectForbidden: 'github-installation-admin-required',
  }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();

    await user.click((await canvas.findAllByRole('button', { name: 'これを使う' }))[0]!);

    await expect(
      canvas.findByText(/GitHub App が見えるすべてのリポジトリの管理者権限が必要です/),
    ).resolves.toBeInTheDocument();
    // 候補は残り、別のものを選べる
    await expect(canvas.getAllByRole('button', { name: 'これを使う' })).toHaveLength(2);
    await expect(canvas.queryByText('連携するリポジトリを選択')).not.toBeInTheDocument();
  },
};

export const Connected: Story = {
  name: '連携済み（リポジトリ名と解除ボタン表示）',
  beforeEach: mockFetch({ connected: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('koyori-app/koyori')).resolves.toBeInTheDocument();
    await expect(canvas.findByRole('button', { name: '連携を解除' })).resolves.toBeInTheDocument();
    await expect(canvas.queryByRole('button', { name: '連携する' })).not.toBeInTheDocument();
  },
};

export const DisconnectFlow: Story = {
  name: '解除フロー（確認ダイアログ → DELETE → 未連携表示）',
  beforeEach: mockFetch({ connected: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const page = within(canvasElement.ownerDocument.body);
    const user = userEvent.setup();

    await user.click(await canvas.findByRole('button', { name: '連携を解除' }));
    await expect(page.findByText('GitHub 連携を解除しますか？')).resolves.toBeInTheDocument();
    await user.click(page.getByRole('button', { name: '解除する' }));

    await waitFor(async () => {
      const del = (fetchSpy!.mock.calls as [Request | string][])
        .map(([req]) => req)
        .filter((req): req is Request => typeof req !== 'string')
        .find((req) => req.method === 'DELETE');
      await expect(del).toBeTruthy();
      await expect(del!.url).toContain(
        `/tenants/${TENANT_UUID}/projects/${PROJECT_UUID}/github/integration`,
      );
    });
    // 再取得後は未連携カードに戻る
    await expect(canvas.findByRole('button', { name: '連携する' })).resolves.toBeInTheDocument();
  },
};

export const ImportIssues: Story = {
  name: 'Issue 取り込み（POST /github/import → 開始メッセージ）',
  beforeEach: mockFetch({ connected: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();

    await user.click(await canvas.findByRole('button', { name: 'Issue を取り込む' }));

    await waitFor(async () => {
      const post = (fetchSpy!.mock.calls as [Request | string][])
        .map(([req]) => req)
        .filter((req): req is Request => typeof req !== 'string')
        .find((req) => req.url.includes('/github/import'));
      await expect(post).toBeTruthy();
      await expect(post!.url).toContain(
        `/tenants/${TENANT_UUID}/projects/${PROJECT_UUID}/github/import`,
      );
    });
    await expect(canvas.findByText(/Issue の取り込みを開始しました/)).resolves.toBeInTheDocument();
  },
};

export const LoadError: Story = {
  name: '状態取得エラー（再試行ボタン表示）',
  beforeEach: mockFetch({ failIntegration: true }),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.findByText('連携状態を取得できませんでした')).resolves.toBeInTheDocument();
    await expect(canvas.findByRole('button', { name: '再試行' })).resolves.toBeInTheDocument();
  },
};
