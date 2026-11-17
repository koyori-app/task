import { describe, expect, it } from 'vitest';
import { setChecklistHtmlChecked, toggleChecklistAt } from '../task-checklist-toggle';

describe('toggleChecklistAt', () => {
  it('付ける・外す', () => {
    expect(toggleChecklistAt('- [ ] a', 2, true)).toBe('- [x] a');
    expect(toggleChecklistAt('- [x] a', 2, false)).toBe('- [ ] a');
  });

  it('[X] を外す・タブの [\\t] を付ける', () => {
    expect(toggleChecklistAt('- [X] a', 2, false)).toBe('- [ ] a');
    expect(toggleChecklistAt('- [\t] a', 2, true)).toBe('- [x] a');
  });

  it('同じ状態への指定は同じ文字列', () => {
    expect(toggleChecklistAt('- [x] a', 2, true)).toBe('- [x] a');
    expect(toggleChecklistAt('- [ ] a', 2, false)).toBe('- [ ] a');
  });

  it('CRLF の本文で 2 行目以降の項目が正しく替わり、\\r\\n が残る', () => {
    const src = '- [ ] a\r\n- [ ] b\r\n- [ ] c';
    // LF 換算の位置: 2, 10, 18
    expect(toggleChecklistAt(src, 10, true)).toBe('- [ ] a\r\n- [x] b\r\n- [ ] c');
    expect(toggleChecklistAt(src, 18, true)).toBe('- [ ] a\r\n- [ ] b\r\n- [x] c');
  });

  it('`[` でない位置・本文の長さちょうど・長さを越える・負の値・整数でない値は null', () => {
    const src = '- [ ] a';
    expect(toggleChecklistAt(src, 0, true)).toBeNull();
    expect(toggleChecklistAt(src, 3, true)).toBeNull();
    expect(toggleChecklistAt(src, src.length, true)).toBeNull();
    expect(toggleChecklistAt(src, src.length + 5, true)).toBeNull();
    expect(toggleChecklistAt(src, -1, true)).toBeNull();
    expect(toggleChecklistAt(src, 2.5, true)).toBeNull();
  });

  it('`[` の中の字が不正・`]` が無ければ null', () => {
    expect(toggleChecklistAt('- [y] a', 2, true)).toBeNull();
    expect(toggleChecklistAt('- [ x a', 2, true)).toBeNull();
    expect(toggleChecklistAt('- [', 2, true)).toBeNull();
  });
});

describe('setChecklistHtmlChecked', () => {
  const html =
    '<ul class="contains-task-list">\n' +
    '<li class="task-list-item"><input type="checkbox" checked="" disabled="" data-kfm-task-offset="2"> a</li>\n' +
    '<li class="task-list-item"><input type="checkbox" disabled="" data-kfm-task-offset="12"> b</li>\n' +
    '<li class="task-list-item"><input type="checkbox" disabled="" data-kfm-task-offset="120"> c</li>\n' +
    '</ul>';

  it('対象の input だけが変わる', () => {
    const on = setChecklistHtmlChecked(html, 12, true);
    expect(on).toBe(
      html.replace(
        'disabled="" data-kfm-task-offset="12"',
        'checked="" disabled="" data-kfm-task-offset="12"',
      ),
    );
    const off = setChecklistHtmlChecked(html, 2, false);
    expect(off).toBe(
      html.replace(
        'checked="" disabled="" data-kfm-task-offset="2"',
        'disabled="" data-kfm-task-offset="2"',
      ),
    );
  });

  it('添字が前方一致する別の項目 (12 と 120) を取り違えない', () => {
    const on = setChecklistHtmlChecked(html, 120, true)!;
    expect(on).toContain('checked="" disabled="" data-kfm-task-offset="120"');
    expect(on).toContain('<input type="checkbox" disabled="" data-kfm-task-offset="12">');
  });

  it('無い添字は null', () => {
    expect(setChecklistHtmlChecked(html, 99, true)).toBeNull();
    expect(setChecklistHtmlChecked(html, -1, true)).toBeNull();
  });

  it('本文の字が同じ並びを含んでいても input の属性だけを見る', () => {
    const tricky =
      '<p>data-kfm-task-offset="5"</p><input type="checkbox" disabled="" data-kfm-task-offset="5">';
    expect(setChecklistHtmlChecked(tricky, 5, true)).toBe(
      '<p>data-kfm-task-offset="5"</p><input type="checkbox" checked="" disabled="" data-kfm-task-offset="5">',
    );
  });
});
