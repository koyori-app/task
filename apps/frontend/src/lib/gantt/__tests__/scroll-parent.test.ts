import { afterEach, describe, expect, it } from 'vitest';
import { scrollParentOf } from '../scroll-parent';

afterEach(() => {
  document.body.innerHTML = '';
});

describe('scrollParentOf', () => {
  it('縦に巻く最も近い祖先を返す', () => {
    document.body.innerHTML =
      '<main id="outer" style="overflow-y: auto"><div id="mid" style="overflow-y: scroll"><div style="overflow-x: auto"><div id="box"></div></div></div></main>';
    expect(scrollParentOf(document.getElementById('box'))).toBe(document.getElementById('mid'));
  });

  it('横にだけ巻く祖先は飛ばす', () => {
    document.body.innerHTML =
      '<main id="outer" style="overflow-y: auto"><div style="overflow-x: auto"><div id="box"></div></div></main>';
    expect(scrollParentOf(document.getElementById('box'))).toBe(document.getElementById('outer'));
  });

  it('巻く祖先が無ければ window', () => {
    document.body.innerHTML = '<div><div id="box"></div></div>';
    expect(scrollParentOf(document.getElementById('box'))).toBe(window);
    expect(scrollParentOf(null)).toBe(window);
  });
});
