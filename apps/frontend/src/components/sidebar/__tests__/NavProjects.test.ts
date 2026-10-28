import { describe, it, expect, afterEach } from 'vitest';
import { mount, enableAutoUnmount } from '@vue/test-utils';
import type { components } from '@/generated/api';

type ProjectNavItem = components['schemas']['ProjectResponse'];
import NavProjects from '../NavProjects.vue';

enableAutoUnmount(afterEach);

const tenantSlug = 'acme';

const sampleProjects: ProjectNavItem[] = [
  {
    id: '00000000-0000-4000-8000-000000000010',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'Team Alpha',
    description: 'Shared project',
    key: 'ALPHA',
    is_personal: false,
    icon_emoji: null,
    icon_url: null,
    personal_owner_id: null,
  },
  {
    id: '00000000-0000-4000-8000-000000000020',
    tenant_id: '00000000-0000-4000-8000-000000000001',
    name: 'Personal',
    description: 'Personal project',
    key: 'ME01',
    is_personal: true,
    icon_emoji: '📝',
    icon_url: null,
    personal_owner_id: '00000000-0000-4000-8000-000000000099',
  },
];

function mountNavProjects(props: {
  projects?: ProjectNavItem[];
  loading?: boolean;
  error?: boolean;
  tenantSlug?: string;
  currentPath?: string;
}) {
  return mount(
    {
      components: { NavProjects },
      template: `
        <NavProjects
          :tenant-slug="tenantSlug"
          :projects="projects"
          :current-path="currentPath"
          :loading="loading"
          :error="error"
          @retry="onRetry"
          @create="onCreate"
        />
      `,
      data() {
        return {
          tenantSlug: props.tenantSlug ?? tenantSlug,
          projects: props.projects ?? sampleProjects,
          currentPath: props.currentPath ?? '',
          loading: props.loading ?? false,
          error: props.error ?? false,
          retried: false,
          created: false,
        };
      },
      methods: {
        onRetry(this: { retried: boolean }) {
          this.retried = true;
        },
        onCreate(this: { created: boolean }) {
          this.created = true;
        },
      },
    },
    { attachTo: document.body },
  );
}

function findRowButton(wrapper: ReturnType<typeof mountNavProjects>, name: string) {
  return wrapper.findAll('button').find((b) => b.text().includes(name));
}

/** 畳んだ Accordion の子リンクは hidden の中に残るので、見えているものだけを数える */
function visibleHrefs(wrapper: ReturnType<typeof mountNavProjects>) {
  return wrapper
    .findAll('a')
    .filter((a) => a.isVisible())
    .map((a) => a.attributes('href'));
}

function findChildLink(wrapper: ReturnType<typeof mountNavProjects>, href: string) {
  return wrapper.findAll('a').find((a) => a.attributes('href') === href);
}

describe('NavProjects', () => {
  it('プロジェクト行はトグルボタンで、リンクは持たない', () => {
    const wrapper = mountNavProjects({});
    expect(findRowButton(wrapper, 'Team Alpha')!.attributes('aria-expanded')).toBe('false');
    expect(findRowButton(wrapper, 'Personal')!.attributes('aria-expanded')).toBe('false');
    // 展開前は子リンクが見えない
    expect(visibleHrefs(wrapper)).toEqual([]);
  });

  it('行クリックで展開し、タスク/ラベル/レビュー/設定の子リンクを出す', async () => {
    const wrapper = mountNavProjects({});
    await findRowButton(wrapper, 'Team Alpha')!.trigger('click');

    const hrefs = visibleHrefs(wrapper);
    expect(hrefs).toContain('/acme/projects/ALPHA/tasks');
    expect(hrefs).toContain('/acme/projects/ALPHA/labels');
    expect(hrefs).toContain('/acme/projects/ALPHA/reviews');
    expect(hrefs).toContain('/acme/projects/ALPHA/settings');
    expect(wrapper.text()).toContain('タスク');
    expect(wrapper.text()).toContain('ラベル');
    expect(wrapper.text()).toContain('レビュー');
    expect(wrapper.text()).toContain('設定');
  });

  // 個人プロジェクトは設定に入れない = GitHub 連携を張れないので、レビューも出さない
  it('個人プロジェクトには設定とレビューの子リンクを出さない', async () => {
    const wrapper = mountNavProjects({});
    await findRowButton(wrapper, 'Personal')!.trigger('click');

    const hrefs = wrapper.findAll('a').map((a) => a.attributes('href'));
    expect(hrefs).toContain('/acme/projects/ME01/tasks');
    expect(hrefs).toContain('/acme/projects/ME01/labels');
    expect(hrefs).not.toContain('/acme/projects/ME01/settings');
    expect(hrefs).not.toContain('/acme/projects/ME01/reviews');
  });

  it('現在のパスのプロジェクトは初期展開され、子が現在地になる', () => {
    const wrapper = mountNavProjects({ currentPath: '/acme/projects/ALPHA/tasks' });
    const activeChild = findChildLink(wrapper, '/acme/projects/ALPHA/tasks');
    expect(activeChild!.isVisible()).toBe(true);
    expect(activeChild!.attributes('aria-current')).toBe('page');
    expect(findRowButton(wrapper, 'Team Alpha')!.attributes('aria-expanded')).toBe('true');
    // 非カレントプロジェクトは初期折りたたみ（子リンクが見えない）
    expect(findRowButton(wrapper, 'Personal')!.attributes('aria-expanded')).toBe('false');
    expect(visibleHrefs(wrapper)).not.toContain('/acme/projects/ME01/tasks');
  });

  it('子の active は境界付き前方一致（接頭辞違いの誤マッチなし・配下ページはマッチ）', () => {
    // 配下ページ（タスク詳細）でも「タスク」が active
    const detail = mountNavProjects({ currentPath: '/acme/projects/ALPHA/tasks/ALPHA-1' });
    const tasksChild = findChildLink(detail, '/acme/projects/ALPHA/tasks');
    expect(tasksChild!.attributes('aria-current')).toBe('page');
    // 接頭辞だけ一致する別パスは active にならない
    const prefix = mountNavProjects({ currentPath: '/acme/projects/ALPHA/tasks-archive' });
    const notActive = findChildLink(prefix, '/acme/projects/ALPHA/tasks');
    expect(notActive!.attributes('aria-current')).toBeUndefined();
  });

  it('アイコンの無いプロジェクトは見出しに頭文字を出す', () => {
    const wrapper = mountNavProjects({});
    // 見出しの最初の aria-hidden がアイコンの枠
    const iconOf = (name: string) => findRowButton(wrapper, name)!.find('[aria-hidden="true"]');
    expect(iconOf('Team Alpha').text()).toBe('T');
    expect(iconOf('Personal').text()).toBe('📝');
  });

  it('lists personal projects before shared projects', () => {
    const wrapper = mountNavProjects({});
    const text = wrapper.text();
    expect(text.indexOf('Personal')).toBeLessThan(text.indexOf('Team Alpha'));
  });

  it('shows loading state', () => {
    const wrapper = mountNavProjects({ projects: [], loading: true });
    expect(wrapper.text()).toContain('プロジェクトを読み込み中');
  });

  it('空状態は破線カードで、作成ボタンが create を emit する', async () => {
    const wrapper = mountNavProjects({ projects: [] });
    expect(wrapper.text()).toContain('プロジェクトはまだありません');
    // 見出し横の作成ボタンはアイコンだけ（aria-label）なので、文字を持つのはカードのボタン
    const createButton = wrapper
      .findAll('button')
      .find((b) => b.text().includes('プロジェクトを作成'));
    expect(createButton).toBeTruthy();
    await createButton!.trigger('click');
    expect(wrapper.vm.created).toBe(true);
  });

  it('shows error state and emits retry', async () => {
    const wrapper = mountNavProjects({ projects: [], error: true });
    expect(wrapper.text()).toContain('プロジェクト一覧を取得できませんでした');
    const retryButton = wrapper.findAll('button').find((b) => b.text() === '再試行');
    await retryButton!.trigger('click');
    expect(wrapper.vm.retried).toBe(true);
  });

  it('emits create when the heading button is clicked', async () => {
    const wrapper = mountNavProjects({});
    const createButton = wrapper.find('button[aria-label="プロジェクトを作成"]');
    expect(createButton.exists()).toBe(true);
    await createButton.trigger('click');
    expect(wrapper.vm.created).toBe(true);
  });

  it('hides the create action when tenant slug is empty', () => {
    const wrapper = mountNavProjects({ tenantSlug: '' });
    expect(wrapper.find('button[aria-label="プロジェクトを作成"]').exists()).toBe(false);
  });
});
