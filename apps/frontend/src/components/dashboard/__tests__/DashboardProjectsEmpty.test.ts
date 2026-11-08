import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';
import DashboardProjectsEmpty from '../DashboardProjectsEmpty.vue';

const { navigateSpy } = vi.hoisted(() => ({ navigateSpy: vi.fn() }));
vi.mock('vike/client/router', () => ({ navigate: navigateSpy }));

enableAutoUnmount(afterEach);
beforeEach(() => vi.clearAllMocks());

describe('DashboardProjectsEmpty', () => {
  it('管理者には作成ボタンを出し、押すとプロジェクト作成画面へ遷移する', async () => {
    const wrapper = mount(DashboardProjectsEmpty, {
      props: { tenantSlug: 'acme', canCreateProject: true },
    });
    expect(wrapper.get('button').text()).toBe('プロジェクトを作成');
    await wrapper.get('button').trigger('click');
    expect(navigateSpy).toHaveBeenCalledWith('/acme/projects/new');
  });

  it('管理者でなければ作成ボタンを出さず、管理者へ依頼する案内を出す', () => {
    const wrapper = mount(DashboardProjectsEmpty, {
      props: { tenantSlug: 'acme', canCreateProject: false },
    });
    expect(wrapper.find('button').exists()).toBe(false);
    expect(wrapper.text()).toContain('管理者に依頼してください。');
    expect(navigateSpy).not.toHaveBeenCalled();
  });
});
