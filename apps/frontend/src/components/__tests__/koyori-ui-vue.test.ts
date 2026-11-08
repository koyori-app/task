import { describe, expect, it } from 'vitest';
import { mount } from '@vue/test-utils';
import { Button } from '@koyori-app/ui-vue';

/*
 * @koyori-app/ui-vue の導入ガード。依存の解決とバンドルの読み込みが壊れると
 * ここで落ちる (部品の置き換えはこの後のサブタスクで行う)。
 */
describe('@koyori-app/ui-vue', () => {
  it('Button を label 付きで描くと、既定の primary の button 要素になる', () => {
    const wrapper = mount(Button, { props: { label: '保存' } });

    const button = wrapper.get('button');
    expect(button.attributes('data-variant')).toBe('primary');
    expect(button.text()).toContain('保存');
  });
});
