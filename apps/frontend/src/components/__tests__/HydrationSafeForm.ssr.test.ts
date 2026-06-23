import { createSSRApp, h } from 'vue';
import { renderToString } from 'vue/server-renderer';
import { describe, expect, it } from 'vitest';

import HydrationSafeForm from '../HydrationSafeForm.vue';

describe('HydrationSafeForm SSR', () => {
  it('renders an intrinsic prehydration submit guard', async () => {
    const app = createSSRApp({
      render: () =>
        h(HydrationSafeForm, null, {
          default: () => h('button', { type: 'submit' }, 'Submit'),
        }),
    });

    const html = await renderToString(app);

    expect(html).toContain('onsubmit="return false;"');
  });
});
