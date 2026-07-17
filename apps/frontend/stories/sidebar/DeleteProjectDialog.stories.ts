import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, fn, userEvent, within } from 'storybook/test';
import { provide } from 'vue';
import { QueryClient, VUE_QUERY_CLIENT } from '@tanstack/vue-query';
import DeleteProjectDialog from '@/components/sidebar/DeleteProjectDialog.vue';
import type { components } from '@/generated/api';

type ProjectResponse = components['schemas']['ProjectResponse'];

const TENANT_UUID = '11111111-1111-1111-1111-111111111111';

const sampleProject: ProjectResponse = {
  id: '00000000-0000-4000-8000-000000000010',
  tenant_id: TENANT_UUID,
  name: 'Team Alpha',
  description: '',
  key: 'ALPHA',
  is_personal: false,
  icon_emoji: null,
  icon_url: null,
  personal_owner_id: null,
};

let fetchSpy: ReturnType<typeof fn> | null = null;

function mockFetch(status = 204) {
  return () => {
    const original = globalThis.fetch;
    fetchSpy = fn().mockImplementation(async () => {
      if (status === 204) return new Response(null, { status });
      return new Response(JSON.stringify({ message: 'error' }), {
        status,
        headers: { 'Content-Type': 'application/json' },
      });
    });
    globalThis.fetch = fetchSpy;
    return () => {
      globalThis.fetch = original;
      fetchSpy = null;
    };
  };
}

function storyRender() {
  return () => ({
    components: { DeleteProjectDialog },
    setup() {
      const queryClient = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      provide(VUE_QUERY_CLIENT, queryClient);
      return { sampleProject };
    },
    template: `<DeleteProjectDialog :open="true" tenant-id="${TENANT_UUID}" :project="sampleProject" @update:open="() => {}" />`,
  });
}

const meta = {
  title: 'Sidebar/DeleteProjectDialog',
  component: DeleteProjectDialog,
  parameters: { layout: 'fullscreen' },
} satisfies Meta<typeof DeleteProjectDialog>;

export default meta;
type Story = StoryObj;

export const Confirm: Story = {
  name: '削除確認→DELETE',
  beforeEach: mockFetch(),
  render: storyRender(),
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await expect(
      await page.findByText('「Team Alpha」を削除します。この操作は取り消せません。'),
    ).toBeInTheDocument();
    await userEvent.click(page.getByRole('button', { name: '削除する' }));
    const req = (fetchSpy!.mock.calls as [Request][])[0]?.[0];
    await expect(req.method).toBe('DELETE');
    await expect(req.url).toContain(`/projects/${sampleProject.id}`);
  },
};

export const Failure: Story = {
  name: '削除失敗',
  beforeEach: mockFetch(403),
  render: storyRender(),
  play: async ({ canvasElement }) => {
    const page = within(canvasElement.ownerDocument.body);
    await userEvent.click(page.getByRole('button', { name: '削除する' }));
    await expect(await page.findByRole('alert')).toHaveTextContent(
      'プロジェクトを削除できませんでした',
    );
  },
};
