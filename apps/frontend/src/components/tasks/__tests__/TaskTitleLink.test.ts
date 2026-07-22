import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

const { navigateSpy } = vi.hoisted(() => ({ navigateSpy: vi.fn() }));

vi.mock('vike/client/router', () => ({
  navigate: navigateSpy,
}));

import TaskTitleLink from '../TaskTitleLink.vue';

enableAutoUnmount(afterEach);

function mountLink(props: Record<string, unknown> = {}) {
  return mount(TaskTitleLink, {
    props: {
      tenantDisplayId: 'acme',
      projectKey: 'ENG',
      seqId: 42,
      title: 'タイトル',
      ...props,
    },
  });
}

describe('TaskTitleLink', () => {
  beforeEach(() => {
    navigateSpy.mockReset();
  });

  it('inlineSelect 無し: 素の左クリックで詳細ページへ navigate する', async () => {
    const wrapper = mountLink();
    await wrapper.get('a').trigger('click', { button: 0 });
    expect(navigateSpy).toHaveBeenCalledTimes(1);
    expect(navigateSpy).toHaveBeenCalledWith('/acme/projects/ENG/tasks/ENG-42');
    expect(wrapper.emitted('select')).toBeFalsy();
  });

  it('inlineSelect 有り: 素の左クリックで select を emit し navigate しない', async () => {
    const wrapper = mountLink({ inlineSelect: true });
    await wrapper.get('a').trigger('click', { button: 0 });
    expect(navigateSpy).not.toHaveBeenCalled();
    expect(wrapper.emitted('select')).toEqual([[42]]);
  });

  it('修飾キー付きクリックは navigate も select もせず href（フルページ）に委ねる', async () => {
    const wrapper = mountLink({ inlineSelect: true });
    await wrapper.get('a').trigger('click', { button: 0, metaKey: true });
    expect(navigateSpy).not.toHaveBeenCalled();
    expect(wrapper.emitted('select')).toBeFalsy();
  });
});
