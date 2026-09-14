import { describe, it, expect, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import ReviewFindingBody from '../ReviewFindingBody.vue';

/*
 * 指摘の本文は書き手の入れた文字であり、KFM (markup-renderer) の既存経路で
 * 描く。ここで固定するのは三つ:
 *   1. 実際に出ていなかった印 (码・链) が出ること
 *   2. 危うい印が sanitize で無害化されること (陽性対照つき)
 *   3. 段落 (空行区切り) の見え方が保たれること
 * 試料は実物の指摘の流儀 (honden-review 投入仕様: 説明 + → 対処、`識別子`、
 * [file:line](URL) の链、空行で段落) に合わせている。
 */

async function mountAndRender(body: string) {
  const wrapper = mount(ReviewFindingBody, {
    props: { body, findingId: 'f-1' },
  });
  // 動的 import → 非同期描画の完了を待つ (完了までは素のテキスト表示)。
  // 初回は KFM 一式の import が挟まるため、既定 1s では足りないことがある
  await vi.waitFor(
    () => {
      expect(wrapper.find('div[data-review-finding-body]').exists()).toBe(true);
    },
    { timeout: 10_000 },
  );
  return wrapper;
}

describe('ReviewFindingBody', () => {
  it('backtick で囲んだ語が码として出る', async () => {
    const wrapper = await mountAndRender(
      '`revoke_all_personal_tokens` は `payload.confirm_tenant_id` を検めずに進む。',
    );
    const codes = wrapper.findAll('code').map((c) => c.text());
    expect(codes).toContain('revoke_all_personal_tokens');
    expect(codes).toContain('payload.confirm_tenant_id');
  });

  it('[題](url) が押せる链として出る', async () => {
    const wrapper = await mountAndRender(
      '→ [handlers/personal_tokens.rs:248](https://github.com/koyori-app/task/blob/main/apps/backend/crates/handler/src/handlers/personal_tokens.rs#L248) の判定を揃えること。',
    );
    const link = wrapper.find('a');
    expect(link.exists()).toBe(true);
    expect(link.attributes('href')).toBe(
      'https://github.com/koyori-app/task/blob/main/apps/backend/crates/handler/src/handlers/personal_tokens.rs#L248',
    );
    expect(link.text()).toBe('handlers/personal_tokens.rs:248');
  });

  it('危うい印は sanitize で消え、同じ本文の普通の印は出る (陽性対照)', async () => {
    const wrapper = await mountAndRender(
      [
        'この指摘は **重要** である。',
        '',
        '<script>alert(1)</script>',
        '',
        '<img src="x" onerror="alert(1)">',
        '',
        '[危うい链](javascript:alert(1)) と [安全な链](https://example.com/fix)',
      ].join('\n'),
    );
    const html = wrapper.find('div[data-review-finding-body]').element.innerHTML;
    // 危うい物は残らない
    expect(html).not.toContain('<script');
    expect(html).not.toContain('onerror');
    expect(html).not.toContain('javascript:');
    // 陽性対照: 同じ経路で普通の印は出る (無害化が効きすぎて空になっていない)
    expect(wrapper.find('strong').text()).toBe('重要');
    const hrefs = wrapper.findAll('a').map((a) => a.attributes('href'));
    expect(hrefs).toContain('https://example.com/fix');
  });

  it('空行で区切った段落が段落のまま出る', async () => {
    const wrapper = await mountAndRender(
      ['一段目の説明である。', '', '二段目の根拠である。', '', '→ 三段目の対処である。'].join('\n'),
    );
    const paragraphs = wrapper.findAll('div[data-review-finding-body] p').map((p) => p.text());
    expect(paragraphs).toEqual([
      '一段目の説明である。',
      '二段目の根拠である。',
      '→ 三段目の対処である。',
    ]);
  });

  it('外への链に既存の描画が付ける属性のまま出る (rel / target は足さない)', async () => {
    const wrapper = await mountAndRender('[出典](https://example.com/evidence)');
    const link = wrapper.find('a');
    expect(link.attributes('href')).toBe('https://example.com/evidence');
    // 既存の描画 (タスク説明と同じ経路) は target / rel を付けない。
    // 変えるなら markup-renderer 側で全消費側を揃えて変えるべきで、ここでは足さない。
    expect(link.attributes('target')).toBeUndefined();
  });
});
