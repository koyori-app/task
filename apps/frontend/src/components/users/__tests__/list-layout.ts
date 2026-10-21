import { afterEach, beforeEach } from 'vitest';

/**
 * happy-dom は配置を持たず、要素の丈が 0 になる。候補の一覧は仮想化しており、丈が 0 だと
 * 一項も描かれない。試験の間だけ listbox（一覧の送り手）に丈を与える。
 *
 * 丈は UserSelectorList の max-h-72（288px）に合わせる。項は 32px なので 9 項が見え、
 * 前後の余り（reka の既定で 12 項）と合わせて描かれる。
 *
 * 一覧を送る動き（End で末尾へ届くなど）は配置が要るので、ここでは確かめない。
 * Chromium の story（`Components/UserSelector` の ManyUsers・HugeUsers）で確かめる。
 */
export function useListLayout(height = 288) {
  const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetHeight');

  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
      configurable: true,
      get(this: HTMLElement) {
        return this.getAttribute('role') === 'listbox' ? height : 0;
      },
    });
  });

  afterEach(() => {
    if (original) Object.defineProperty(HTMLElement.prototype, 'offsetHeight', original);
    // 元が Element.prototype の側に在れば、HTMLElement の側を消すとそれに戻る
    else Reflect.deleteProperty(HTMLElement.prototype, 'offsetHeight');
  });
}
