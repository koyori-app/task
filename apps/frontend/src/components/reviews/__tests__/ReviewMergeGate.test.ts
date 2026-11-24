import { afterEach, describe, expect, it } from 'vitest';
import { enableAutoUnmount, mount } from '@vue/test-utils';

import ReviewMergeGate from '../ReviewMergeGate.vue';
import type { ForgeTarget } from '@/lib/forge-host';
import type { ReviewSummary } from '@/lib/review-findings';
import { summary } from './review-fixtures';

const TARGET: ForgeTarget = { host: 'github', html_url: null, repository: 'acme/app' };
const HEAD = 'a1b2c3d4e5f60718293a4b5c6d7e8f9012345678';

function mountGate(overrides: Partial<ReviewSummary>, target: ForgeTarget = TARGET) {
  return mount(ReviewMergeGate, { props: { summary: summary(overrides), target } });
}

enableAutoUnmount(afterEach);

describe('ReviewMergeGate', () => {
  it('ready は見出しと確認時刻だけで、条件の行を出さない', () => {
    const wrapper = mountGate({ gate: 'ready' });
    expect(wrapper.get('h2').text()).toBe('マージ可');
    expect(wrapper.text()).toMatch(/GitHub 確認: .+ 時点/);
    expect(wrapper.find('[data-testid="merge-gate-condition"]').exists()).toBe(false);
    expect(wrapper.find('button').exists()).toBe(false);
  });

  it('blocked は満たしていない条件だけを出し、「未解決を見る」を emit する', async () => {
    const wrapper = mountGate({
      gate: 'blocked',
      blocking: 4,
      counts: [
        { severity: 'high', state: 'open', count: 3 },
        { severity: 'medium', state: 'fixed', count: 1 },
      ],
    });
    expect(wrapper.get('h2').text()).toBe('マージ不可');
    expect(wrapper.get('[data-testid="merge-gate-condition"]').text()).toContain(
      'High・Medium の未解決が 4 件（うち承認待ち 1）',
    );
    // 満たしている条件と、条件の説明文は出さない
    expect(wrapper.text()).not.toContain('Low');
    expect(wrapper.text()).not.toContain('GitHub 確認');

    await wrapper.get('button').trigger('click');
    expect(wrapper.get('button').text()).toBe('未解決を見る');
    expect(wrapper.emitted('show-unresolved')).toHaveLength(1);
    expect(wrapper.emitted('compose')).toBeUndefined();
  });

  it('unreviewed は「指摘を起票」を emit する', async () => {
    const wrapper = mountGate({ gate: 'unreviewed', rounds: 0 });
    expect(wrapper.get('h2').text()).toBe('未レビュー');
    expect(wrapper.text()).toContain('まだレビューがありません');
    await wrapper.get('button').trigger('click');
    expect(wrapper.get('button').text()).toBe('指摘を起票');
    expect(wrapper.emitted('compose')).toHaveLength(1);
  });

  it('outdated は今の head をホストのコミットへのリンクにする', async () => {
    const wrapper = mountGate({ gate: 'outdated', cached_pr_head_sha: HEAD });
    expect(wrapper.get('h2').text()).toBe('レビューが古い');
    const link = wrapper.get('a');
    expect(link.text()).toBe('a1b2c3d');
    expect(link.attributes('href')).toBe(`https://github.com/acme/app/commit/${HEAD}`);
    expect(link.attributes('title')).toBe(HEAD);
    expect(wrapper.get('[data-testid="merge-gate-condition"]').text()).toMatch(
      /レビューのあとにコミットが積まれています（今は\s*a1b2c3d\s*）/,
    );
    await wrapper.get('button').trigger('click');
    expect(wrapper.emitted('compose')).toHaveLength(1);
  });

  it('outdated でコミットの URL が組めなければ、ハッシュを文字だけで出す', () => {
    const wrapper = mountGate(
      { gate: 'outdated', cached_pr_head_sha: HEAD },
      { host: null, html_url: null, repository: null },
    );
    expect(wrapper.find('a').exists()).toBe(false);
    expect(wrapper.text()).toContain('a1b2c3d');
  });

  it('stale_unknown と unlinked はボタンを出さない', () => {
    const stale = mountGate({ gate: 'stale_unknown', cached_pr_head_sha: null });
    expect(stale.get('h2').text()).toBe('鮮度不明');
    expect(stale.text()).toContain('今の head を GitHub で確かめられていません');
    expect(stale.find('button').exists()).toBe(false);

    const unlinked = mountGate({ gate: 'unlinked', repository: null });
    expect(unlinked.get('h2').text()).toBe('リポジトリ未確定');
    expect(unlinked.text()).toContain(
      'GitHub と連携していないため、どの PR を見た集計か決まりません',
    );
    expect(unlinked.find('button').exists()).toBe(false);
  });

  it('オーナー代行での棄却は、件数があるときだけ出す', () => {
    expect(mountGate({ gate: 'ready' }).find('[data-testid="merge-gate-override"]').exists()).toBe(
      false,
    );
    expect(
      mountGate({ gate: 'ready', owner_override_rejections: 2 })
        .get('[data-testid="merge-gate-override"]')
        .text(),
    ).toBe('オーナー代行での棄却 2 件');
  });
});
