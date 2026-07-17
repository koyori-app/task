import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import ProjectFormDialog from '@/components/sidebar/ProjectFormDialog.vue';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleProject: ProjectResponse = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: 'Shared project',
  key: 'ALPHA',
  is_personal: false,
  icon_emoji: null,
  icon_url: null,
  personal_owner_id: null,
};

const jsonResponse = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });

type MockOptions = { rejectWrite?: number };

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch(overrides: MockOptions = {}) {
  return () => {
    const original = globalThis.fetch;
    fetchSpy = fn().mockImplementation(async (req: Request | string) => {
      const url = typeof req === 'string' ? req : req.url;
      const method = typeof req === 'string' ? 'GET' : req.method;
      if ((method === 'POST' || method === 'PUT') && url.includes('/projects')) {
        if (overrides.rejectWrite) {
          return jsonResponse({ message: 'error' }, overrides.rejectWrite);
        }
        const body = await (req as Request).json();
        return jsonResponse(
          { ...sampleProject, ...body, id: sampleProject.id },
          method === 'POST' ? 201 : 200,
        );
      }
      return jsonResponse([]);
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
    };
  };
}

function storyRender(props: { project?: ProjectResponse | null } = {}) {
  return () => ({
    components: { ProjectFormDialog },
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      return { props };
    },
    template: `<ProjectFormDialog :open="true" tenant-id="${TENANT_UUID}" :project="props.project" @update:open="() => {}" />`,
  });
}

const meta = {
  title: 'Sidebar/ProjectFormDialog',
  component: ProjectFormDialog,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof ProjectFormDialog>;

export default meta;
type Story = StoryObj;

export const Create: Story = {
  name: '作成（キー自動提案）',
  beforeEach: mockFetch(),
  render: storyRender(),
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.type(page.getByLabelText('名前'), 'New Project');
    await expect(page.getByLabelText('キー（任意）')).toHaveValue('NEWPROJECT');
    await userEvent.click(page.getByRole('button', { name: '作成' }));
    const calls = (fetchSpy!.mock.calls as [Request][]).map(([req]) => req);
    const post = calls.find((req) => req.method === 'POST');
    await expect(post).toBeTruthy();
    await expect(post!.url).toContain(`/v1/tenants/${TENANT_UUID}/projects`);
  },
};

export const Edit: Story = {
  name: '編集（キーは変更不可）',
  beforeEach: mockFetch(),
  render: storyRender({ project: sampleProject }),
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await expect(page.getByLabelText('名前')).toHaveValue('Team Alpha');
    await expect(page.getByLabelText('キー')).toBeDisabled();
    await expect(page.getByLabelText('キー')).toHaveValue('ALPHA');
    await userEvent.clear(page.getByLabelText('名前'));
    await userEvent.type(page.getByLabelText('名前'), 'Renamed');
    await userEvent.click(page.getByRole('button', { name: '保存' }));
    const put = (fetchSpy!.mock.calls as [Request][])
      .map(([req]) => req)
      .find((req) => req.method === 'PUT');
    await expect(put).toBeTruthy();
    await expect(put!.url).toContain(`/projects/${sampleProject.id}`);
  },
};

export const CreateFailure: Story = {
  name: '作成失敗',
  beforeEach: mockFetch({ rejectWrite: 500 }),
  render: storyRender(),
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.type(page.getByLabelText('名前'), 'X Project');
    await userEvent.click(page.getByRole('button', { name: '作成' }));
    await expect(await page.findByRole('alert')).toHaveTextContent(
      'プロジェクトを作成できませんでした',
    );
  },
};
