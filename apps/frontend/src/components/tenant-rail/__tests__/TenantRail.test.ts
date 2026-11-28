import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import type { Tenant } from '@/stores/tenant';
import TenantRail from '../TenantRail.vue';

enableAutoUnmount(afterEach);

const alpha: Tenant = {
  id: 'tenant-1',
  display_id: 'alpha',
  name: 'Alpha',
  description: '',
  icon_url: 'https://example.test/alpha.png',
  icon_emoji: '🚀',
  owner_id: 'owner-1',
  require_2fa: false,
  membership: 'Owner',
};
const beta: Tenant = {
  ...alpha,
  id: 'tenant-2',
  display_id: 'beta',
  name: 'Beta',
  icon_url: '',
  icon_emoji: null,
  membership: 'Member',
};

function mountRail(props: Partial<InstanceType<typeof TenantRail>['$props']> = {}) {
  return mount(TenantRail, {
    attachTo: document.body,
    props: { tenants: [alpha, beta], selectedTenantId: alpha.id, ...props },
  });
}

function item(wrapper: ReturnType<typeof mountRail>, label: string) {
  const button = wrapper.find(`button[aria-label="${label}"]`);
  expect(button.exists(), `${label} のボタン`).toBe(true);
  return button;
}

describe('TenantRail', () => {
  it('is a navigation landmark named テナント', () => {
    const wrapper = mountRail();

    expect(wrapper.find('nav[aria-label="テナント"]').exists()).toBe(true);
  });

  it('emits the clicked tenant', async () => {
    const wrapper = mountRail();

    await item(wrapper, 'Beta').trigger('click');

    expect(wrapper.emitted('select')).toEqual([[beta]]);
  });

  it('marks only the selected tenant as current', () => {
    const wrapper = mountRail();

    expect(item(wrapper, 'Alpha').attributes('aria-current')).toBe('true');
    expect(item(wrapper, 'Beta').attributes('aria-current')).toBeUndefined();
    expect(item(wrapper, 'テナントを作成').attributes('aria-current')).toBeUndefined();
  });

  it('shows the image over the emoji, and the initial when the tenant has neither', () => {
    const wrapper = mountRail();

    const alphaItem = item(wrapper, 'Alpha');
    expect(alphaItem.find('img').attributes('src')).toBe(alpha.icon_url);
    expect(alphaItem.text()).not.toContain('🚀');
    const betaItem = item(wrapper, 'Beta');
    expect(betaItem.find('img').exists()).toBe(false);
    expect(betaItem.text()).toBe('B');
  });

  it('shows a busy state without items while loading, and the create button is disabled', () => {
    const wrapper = mountRail({ loading: true });

    expect(wrapper.find('button[aria-label="Alpha"]').exists()).toBe(false);
    expect(wrapper.find('[aria-busy="true"]').text()).toContain('テナントを読み込み中…');
    expect(item(wrapper, 'テナントを作成').attributes('disabled')).toBeDefined();
  });

  it('shows a retry button instead of the items on error', async () => {
    const wrapper = mountRail({ error: 'テナント一覧を取得できませんでした' });

    expect(wrapper.find('button[aria-label="Alpha"]').exists()).toBe(false);
    await item(wrapper, 'テナント一覧を取得できませんでした（再試行）').trigger('click');

    expect(wrapper.emitted('retry')).toHaveLength(1);
  });

  it('tells there is no tenant and still lets the user create one', async () => {
    const wrapper = mountRail({ tenants: [], selectedTenantId: null });

    expect(wrapper.text()).toContain('所属テナントがありません');
    const create = item(wrapper, 'テナントを作成');
    expect(create.attributes('disabled')).toBeUndefined();
    await create.trigger('click');

    expect(wrapper.emitted('create')).toHaveLength(1);
  });

  it('marks no tenant as current when the URL tenant is not in the list', () => {
    const wrapper = mountRail({ selectedTenantId: null });

    expect(item(wrapper, 'Alpha').attributes('aria-current')).toBeUndefined();
    expect(item(wrapper, 'Beta').attributes('aria-current')).toBeUndefined();
  });

  it('emits the menu of the tenant at the pointer on right click', async () => {
    const wrapper = mountRail();

    await item(wrapper, 'Beta').trigger('contextmenu', { clientX: 12, clientY: 34 });

    expect(wrapper.emitted('menu')).toEqual([[beta, { x: 12, y: 34 }]]);
  });
});
