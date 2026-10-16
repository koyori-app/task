import { describe, expect, it } from 'vitest';
import { renderDescription } from '../markup-renderer';

// tenant null = taskRef を渡さない
const render = (text: string, tenant: string | null = 'acme') =>
  renderDescription(text, tenant === null ? {} : { taskRef: { tenant } });

const link = (ref: string, tenant = 'acme') =>
  `<a href="/${tenant}/projects/${ref.split('-')[0]}/tasks/${ref}" class="kfm-task-ref">${ref}</a>`;

describe('remark-kfm-task-ref (KEY-N の自動リンク)', () => {
  it('KEY-N をタスク詳細へのリンクにする (sanitize を class / 相対 href ごと通る)', async () => {
    expect(await render('関連は TASK-58 です')).toBe(`<p>関連は ${link('TASK-58')} です</p>`);
  });

  it('文頭・文末・全角括弧内・句読点の隣でもリンク化する', async () => {
    expect(await render('TASK-1')).toBe(`<p>${link('TASK-1')}</p>`);
    expect(await render('終わりは ENG-42')).toBe(`<p>終わりは ${link('ENG-42')}</p>`);
    expect(await render('（TASK-58）と、AB2-3。')).toBe(
      `<p>（${link('TASK-58')}）と、${link('AB2-3')}。</p>`,
    );
  });

  it('強調の内側もリンク化する (text ノード単位で分割する)', async () => {
    expect(await render('**TASK-9**')).toBe(`<p><strong>${link('TASK-9')}</strong></p>`);
  });

  it('マッチ規則の外は素のテキスト', async () => {
    for (const text of [
      'task-58',
      'Task-58',
      'T-58',
      'ABCDEFGHIJK-1',
      'TASK-58-foo',
      'TASK-58_x',
      'TASK-58a',
      'foo/TASK-58',
      'xTASK-58',
      '_TASK-58',
      'ABC-TASK-58',
    ]) {
      expect(await render(text)).toBe(`<p>${text}</p>`);
    }
  });

  it('コードスパン・コードブロック内はリンク化しない', async () => {
    expect(await render('`TASK-58`')).toBe('<p><code>TASK-58</code></p>');
    const block = await render('```\nTASK-58\n```');
    expect(block).not.toContain('kfm-task-ref');
    expect(block).toContain('TASK-58');
  });

  it('autolink 済みの URL 内と明示リンク内はリンク化しない', async () => {
    expect(await render('https://example.com/TASK-58')).toBe(
      '<p><a href="https://example.com/TASK-58">https://example.com/TASK-58</a></p>',
    );
    expect(await render('[TASK-58](https://example.com)')).toBe(
      '<p><a href="https://example.com">TASK-58</a></p>',
    );
    expect(await render('[TASK-58]\n\n[TASK-58]: https://example.com')).toBe(
      '<p><a href="https://example.com">TASK-58</a></p>',
    );
  });

  it('alert の本文でもリンク化し、alert 化を壊さない', async () => {
    const html = await render('> [!NOTE]\n> TASK-58 を参照');
    expect(html).toContain('kfm-alert--note');
    expect(html).toContain(link('TASK-58'));
  });

  it('tenant 未指定なら素のテキスト (既存の呼び出し元の出力を変えない)', async () => {
    expect(await render('TASK-58', null)).toBe('<p>TASK-58</p>');
  });

  it('不正な tenant は throw せず素のテキスト', async () => {
    for (const tenant of ['', 'a/b', '../x', 'a b', '-acme', 'a"onclick=', 'a'.repeat(64)]) {
      expect(await render('TASK-58', tenant)).toBe('<p>TASK-58</p>');
    }
    // 上限ちょうど (63 文字) は通る
    expect(await render('TASK-58', 'a'.repeat(63))).toContain('kfm-task-ref');
  });

  it('同じ本文を別テナントで描画すると href が変わる (キャッシュ分離)', async () => {
    const text = 'キャッシュ分離 TASK-58';
    expect(await render(text, 'acme')).toContain(link('TASK-58', 'acme'));
    expect(await render(text, 'other')).toContain(link('TASK-58', 'other'));
    expect(await render(text, null)).not.toContain('kfm-task-ref');
  });
});
