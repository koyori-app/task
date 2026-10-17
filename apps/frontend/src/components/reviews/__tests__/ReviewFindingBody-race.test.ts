import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import ReviewFindingBody from '../ReviewFindingBody.vue';

/*
 * 描画の完了順を入れ替えるため、renderDescription を手で解決する偽物に差し替える。
 * 同じ本文のままテナントだけが変わり、古いテナントの描画が後から終わっても、
 * 新しいテナントの描画結果を上書きしないことを固定する。
 */
type Pending = { tenant: string; resolve: (html: string) => void; reject: (e: Error) => void };
const pending: Pending[] = [];

vi.mock('@/lib/markup-renderer', () => ({
  renderDescription: (_source: string, options: { taskRef: { tenant: string } }) =>
    new Promise<string>((resolve, reject) => {
      pending.push({ tenant: options.taskRef.tenant, resolve, reject });
    }),
}));

async function mountAndSwitchTenant() {
  const wrapper = mount(ReviewFindingBody, {
    props: { body: 'TASK-58', findingId: 'f-1', tenant: 'old' },
  });
  await flushPromises();
  await wrapper.setProps({ tenant: 'new' });
  await flushPromises();
  expect(pending.map((p) => p.tenant)).toEqual(['old', 'new']);
  const [oldRender, newRender] = pending;
  newRender.resolve('<p>new</p>');
  await flushPromises();
  return { wrapper, oldRender };
}

describe('ReviewFindingBody の描画の競り合い', () => {
  beforeEach(() => {
    pending.length = 0;
  });

  it('古いテナントの描画が後から成功しても、新しいテナントの結果を上書きしない', async () => {
    const { wrapper, oldRender } = await mountAndSwitchTenant();
    oldRender.resolve('<p>old</p>');
    await flushPromises();

    expect(wrapper.find('div[data-review-finding-body]').html()).toContain('new');
  });

  it('古いテナントの描画が後から失敗しても、新しいテナントの結果を消さない', async () => {
    const { wrapper, oldRender } = await mountAndSwitchTenant();
    oldRender.reject(new Error('boom'));
    await flushPromises();

    expect(wrapper.find('div[data-review-finding-body]').html()).toContain('new');
  });
});
