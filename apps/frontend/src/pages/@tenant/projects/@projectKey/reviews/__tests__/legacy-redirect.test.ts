import { afterEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { enableAutoUnmount, flushPromises, mount } from '@vue/test-utils';

const { navigate, page } = vi.hoisted(() => ({
  navigate: vi.fn(),
  page: { search: {} as Record<string, string> },
}));
vi.mock('vike/client/router', () => ({ navigate }));
vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({
    routeParams: { tenant: 'acme', projectKey: 'APP' },
    urlParsed: { pathname: '/acme/projects/APP/reviews', search: page.search },
  }),
}));
vi.mock('@/composables/useResolvedTenantId', () => ({
  useResolvedTenantId: () => ({
    tenantId: ref('tenant-id'),
    isTenantNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));
vi.mock('@/composables/useResolvedProjectId', () => ({
  useResolvedProjectId: () => ({
    projectId: ref('project-id'),
    isProjectNotFound: ref(false),
    isResolving: ref(false),
    isError: ref(false),
  }),
}));
// 一覧の中身はここでは見ない。描かれたかどうかと、渡った警告だけを見る代役にする
vi.mock('@/components/reviews/ReviewPullRequestList.vue', async () => {
  const { defineComponent, h } = await import('vue');
  return {
    default: defineComponent({
      name: 'ReviewPullRequestList',
      props: ['initialUrlState', 'initialUrlWarnings'],
      setup: (props) => () =>
        h('div', { 'data-testid': 'pr-list' }, (props.initialUrlWarnings ?? []).join('\n')),
    }),
  };
});

import Page from '../+Page.vue';

function mountPage(search: Record<string, string>) {
  page.search = search;
  return mount(Page, { attachTo: document.body });
}

enableAutoUnmount(afterEach);
afterEach(() => {
  navigate.mockReset();
});

describe('レビューの一覧のページ: 旧 URL の転送', () => {
  it('?pr=N は履歴を積まずに詳細へ転送し、待つ間は一覧を描かない', async () => {
    navigate.mockReturnValue(new Promise(() => {}));
    const wrapper = mountPage({ pr: '231', finding: 'x', q: 'fix', mine: '1' });
    await flushPromises();

    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate).toHaveBeenCalledWith('/acme/projects/APP/reviews/231?finding=x', {
      overwriteLastHistoryEntry: true,
    });
    expect(wrapper.find('[data-testid="pr-list"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="legacy-redirect-failed"]').exists()).toBe(false);
  });

  it('転送に失敗したら、読み込み中のまま止めずに詳細へのリンクを出す', async () => {
    navigate.mockRejectedValue(new Error('chunk load failed'));
    const wrapper = mountPage({ pr: '231', state: 'open' });
    await flushPromises();

    const failed = wrapper.get('[data-testid="legacy-redirect-failed"]');
    expect(failed.get('a').attributes('href')).toBe('/acme/projects/APP/reviews/231?state=open');
    expect(wrapper.find('[data-testid="pr-list"]').exists()).toBe(false);
  });

  it('pr が番号として読めなければ転送せず、一覧に警告を渡す', async () => {
    const wrapper = mountPage({ pr: 'abc' });
    await flushPromises();

    expect(navigate).not.toHaveBeenCalled();
    expect(wrapper.get('[data-testid="pr-list"]').text()).toContain(
      'URL の PR 番号「abc」は正の整数ではないため無視しました。',
    );
  });

  it('pr が無ければ転送せず、一覧を描く', async () => {
    const wrapper = mountPage({ q: 'fix' });
    await flushPromises();

    expect(navigate).not.toHaveBeenCalled();
    expect(wrapper.find('[data-testid="pr-list"]').exists()).toBe(true);
  });
});
