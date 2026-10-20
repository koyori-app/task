import { describe, expect, it } from 'vitest';
import type { MentionTable } from '../comment-mention';
import { renderDescription } from '../markup-renderer';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';
const C = '33333333-3333-4333-8333-333333333333';

const mentions: MentionTable = {
  [A]: { status: 'member', name: 'alice' },
  // 外れた者でも名前が引けておれば名で出る（灰色の札）
  [B]: { status: 'outside', name: 'former' },
  [C]: { status: 'outside', name: null },
};

// table が null なら表を渡さない（既定の表を使うのは undefined の時だけ）
const render = (text: string, table: MentionTable | null = mentions) =>
  renderDescription(text, table === null ? {} : { mentions: table });

const chip = (status: string, label: string) =>
  `<span class="kfm-mention kfm-mention-${status}">${label}</span>`;

describe('remark-kfm-mention (<@利用者ID> の印を札にする)', () => {
  it('印を札にする（id は HTML に出ず、状態は class で運ぶ）', async () => {
    const html = await render(`見て <@${A}> と <@${B}> へ`);
    expect(html).toBe(
      `<p>見て ${chip('member', '@alice')} と ${chip('outside', '@former')} へ</p>`,
    );
    expect(html).not.toContain(A);
    expect(html).not.toContain('&lt;@');
  });

  it('名が引けぬ外れた者・表に無い id は定型の句（id は出ない）', async () => {
    const html = await render(`<@${C}> <@44444444-4444-4444-8444-444444444444>`);
    expect(html).toContain(chip('outside', '@（このプロジェクトの外の利用者）'));
    expect(html).toContain(chip('unknown', '@（不明な利用者）'));
    expect(html).not.toMatch(/[0-9a-f]{8}-[0-9a-f]{4}-/);
  });

  it('行の無い者は gone の札', async () => {
    const html = await render(`<@${A}>`, { [A]: { status: 'gone', name: null } });
    expect(html).toBe(`<p>${chip('gone', '@（退会した利用者）')}</p>`);
  });

  it('ログイン中の member への札だけを強調し、キャッシュでも他の閲覧者と混ぜない', async () => {
    const source = `<@${A}> <@${B}>`;
    const self = await renderDescription(source, {
      scope: 'comment-1',
      profile: 'comment',
      mentions,
      currentUserId: A,
    });
    const otherViewer = await renderDescription(source, {
      scope: 'comment-1',
      profile: 'comment',
      mentions,
      currentUserId: B,
    });
    expect(self).toContain('kfm-mention-member kfm-mention-self');
    expect(otherViewer).not.toContain('kfm-mention-member kfm-mention-self');
    expect(self).not.toBe(otherViewer);
  });

  it('強調・リスト・見出しの内側でも札にする', async () => {
    expect(await render(`**<@${A}>**`)).toBe(`<p><strong>${chip('member', '@alice')}</strong></p>`);
    expect(await render(`- <@${A}>`)).toContain(chip('member', '@alice'));
    expect(await render(`# <@${A}>`)).toContain(chip('member', '@alice'));
  });

  it('コードブロックとインラインコードの中の印は札にせず、現在の名前へ戻す', async () => {
    const inline = await render(`\`<@${A}>\``);
    expect(inline).toBe('<p><code>@alice</code></p>');
    expect(inline).not.toContain('kfm-mention');
    const block = await render(`\`\`\`\n<@${A}>\n\`\`\``);
    expect(block).not.toContain('kfm-mention');
    expect(block).toContain('@alice');
    expect(block).not.toContain(A);
  });

  it('リンクの内側・URL の中は札にしない', async () => {
    const html = await render(`[<@${A}>](https://example.com)`);
    expect(html).not.toContain('kfm-mention');
  });

  it('壊れた印・手で打った @名前・メールアドレスは素の文字', async () => {
    for (const text of [
      `<@${A}`,
      `<@${A.slice(0, 35)}>`,
      `<@${A.replaceAll('-', '')}>`,
      '@alice さん',
      'bob@example.com',
    ]) {
      expect(await render(text), text).not.toContain('kfm-mention');
    }
    expect(await render('@alice さん')).toBe('<p>@alice さん</p>');
  });

  it('表を渡さぬ描画は何も変えない（印は素の文字のまま。他の消費側の描き方を変えない）', async () => {
    const html = await render(`<@${A}>`, null);
    expect(html).not.toContain('kfm-mention');
    expect(html).toContain(A);
  });

  it('表が替われば別の HTML（キャッシュが札の字を取り違えない）', async () => {
    const first = await render(`<@${A}>`, { [A]: { status: 'member', name: 'alice' } });
    const renamed = await render(`<@${A}>`, { [A]: { status: 'member', name: 'alice_renamed' } });
    const left = await render(`<@${A}>`, { [A]: { status: 'outside', name: 'alice' } });
    expect(first).toContain('@alice<');
    expect(renamed).toContain('@alice_renamed');
    expect(left).toContain('kfm-mention-outside');
    expect(first).not.toBe(renamed);
    expect(first).not.toBe(left);
  });

  it('名前の中の HTML は escape される（札の字は text）', async () => {
    const html = await render(`<@${A}>`, {
      [A]: { status: 'member', name: '<img src=x onerror=alert(1)>' },
    });
    expect(html).not.toContain('<img');
    expect(html).toContain('&lt;img');
  });
});
