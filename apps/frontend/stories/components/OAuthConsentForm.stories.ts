import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';

import OAuthConsentForm from '@/components/settings/OAuthConsentForm.vue';

const TENANT_ID = '11111111-1111-1111-1111-111111111111';
const SECOND_TENANT_ID = '44444444-4444-4444-4444-444444444444';
const ALL_SCOPES = [
  'read:tenant',
  'read:project',
  'read:task',
  'write:task',
  'read:milestone',
  'read:sprint',
] as const;

// emit の受け口。Vue の emits は story の args の型に出ないので、モジュールに置いて参照する
const onApprove = fn();
const onDeny = fn();

/** MCP の OAuth の同意画面（`/oauth/authorize`）の中身。ページは枠の中では出さないので、部品で撮る。 */
const meta = {
  title: 'Components/Settings/OAuthConsentForm',
  component: OAuthConsentForm,
  tags: ['autodocs'],
  parameters: {
    layout: 'centered',
    docs: {
      description: {
        component:
          'MCP の OAuth の同意画面の中身。アプリの名前は自己申告なので client_id の host を主に出し、戻り先の host、テナント、scope を選んで許可する。戻り先が利用者の端末（loopback）のときは注意書きを出し、テナント未選択・write:task を外した状態で始める。',
      },
    },
  },
  args: {
    preview: {
      client_id: 'https://claude.ai/oauth/mcp-oauth-client-metadata',
      client_name: 'Claude',
      client_host: 'claude.ai',
      redirect_host: 'claude.ai',
      redirect_is_loopback: false,
      scopes: [...ALL_SCOPES],
    },
    tenants: [{ id: TENANT_ID, name: 'Acme Inc' }],
    username: 'tester',
    pending: false,
    error: null,
  },
  render: (args) => ({
    components: { OAuthConsentForm },
    setup: () => ({ args, onApprove, onDeny }),
    template:
      '<div class="koyori-card w-[28rem] max-w-full [--koyori-card-padding:1.5rem]"><OAuthConsentForm v-bind="args" @approve="onApprove" @deny="onDeny" /></div>',
  }),
  beforeEach: () => {
    onApprove.mockClear();
    onDeny.mockClear();
  },
} satisfies Meta<typeof OAuthConsentForm>;

export default meta;
type Story = StoryObj<typeof meta>;

export const HttpsClient: Story = {
  name: 'https のクライアント（候補 1 件は選択済み、scope は全部チェック済み）',
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(canvas.getByTestId('client-host')).toHaveTextContent('claude.ai');
    await expect(canvas.getByTestId('client-name')).toHaveTextContent('Claude');
    await expect(canvas.getByTestId('redirect-host')).toHaveTextContent('claude.ai');
    await expect(canvas.queryByTestId('loopback-warning')).toBeNull();
    await expect(canvas.getByRole('checkbox', { name: /write:task/ })).toBeChecked();

    await user.click(canvas.getByRole('button', { name: '許可する' }));
    await expect(onApprove).toHaveBeenCalledWith({
      tenantIds: [TENANT_ID],
      scopes: [...ALL_SCOPES],
    });
  },
};

export const LoopbackClient: Story = {
  name: 'loopback のクライアント（注意書き、テナント未選択、write:task を外す）',
  args: {
    preview: {
      client_id: 'https://chatgpt.com/oauth/codex/client.json',
      client_name: 'Codex',
      client_host: 'chatgpt.com',
      redirect_host: '127.0.0.1:53124',
      redirect_is_loopback: true,
      scopes: [...ALL_SCOPES],
    },
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByTestId('loopback-warning')).toHaveTextContent(
      'この端末で動くアプリが接続を求めています。',
    );
    await expect(canvas.getByTestId('redirect-host')).toHaveTextContent('127.0.0.1:53124');
    await expect(canvas.getByRole('checkbox', { name: /write:task/ })).not.toBeChecked();
    await expect(canvas.getByRole('checkbox', { name: /read:task/ })).toBeChecked();
    // 候補が 1 件でも未選択で、選ぶまで許可できない
    await expect(canvas.getByRole('button', { name: '許可する' })).toBeDisabled();
    await expect(onApprove).not.toHaveBeenCalled();
  },
};

export const SeveralTenants: Story = {
  name: '候補が 2 件（未選択から始める）',
  args: {
    tenants: [
      { id: TENANT_ID, name: 'Acme Inc' },
      { id: SECOND_TENANT_ID, name: 'Beta LLC' },
    ],
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('button', { name: '許可する' })).toBeDisabled();
    await expect(canvas.getByRole('button', { name: '拒否する' })).toBeEnabled();
  },
};

export const NoTenant: Story = {
  name: '接続できるテナントが無い（許可ボタンを出さない）',
  args: { tenants: [] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const user = userEvent.setup();
    await expect(canvas.getByTestId('no-tenants')).toHaveTextContent(
      '接続できるテナントがありません。テナントに所属すると接続できます。',
    );
    await expect(canvas.queryByRole('button', { name: '許可する' })).toBeNull();
    await user.click(canvas.getByRole('button', { name: '拒否する' }));
    await expect(onDeny).toHaveBeenCalled();
  },
};

export const SubmitError: Story = {
  name: '許可の失敗（2 段階認証を必須にしたテナント）',
  args: {
    error:
      '選択したテナントのどれかが 2 段階認証を必須にしています。2 段階認証を設定してから、もう一度お試しください。',
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('alert')).toHaveTextContent('2 段階認証を必須にしています');
  },
};

export const Pending: Story = {
  name: '送信中（ボタンを押せない）',
  args: { pending: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByRole('button', { name: '送信中…' })).toBeDisabled();
    await expect(canvas.getByRole('button', { name: '拒否する' })).toBeDisabled();
  },
};
