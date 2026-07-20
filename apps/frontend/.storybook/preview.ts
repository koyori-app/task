import type { Preview } from '@storybook/vue3-vite';
// @ts-expect-error — Storybook CSS side-effect import (tailwind)
import '@/assets/css/tailwind.css';

const MOCKED_NOW = '2026-07-15T09:00:00+09:00';

const preview: Preview = {
  beforeEach: () => {
    const SystemDate = globalThis.Date;
    const fixedTimestamp = SystemDate.parse(MOCKED_NOW);
    const FrozenDate = new Proxy(SystemDate, {
      apply: (Target) => new Target(fixedTimestamp).toString(),
      construct: (Target, args, NewTarget) =>
        Reflect.construct(Target, args.length === 0 ? [fixedTimestamp] : args, NewTarget),
    });

    globalThis.Date = FrozenDate;

    return () => {
      globalThis.Date = SystemDate;
    };
  },
  parameters: {
    viewport: {
      defaultViewport: 'desktop1440',
      viewports: {
        desktop1440: {
          name: 'Desktop 1440',
          styles: { width: '1440px', height: '900px' },
          type: 'desktop',
        },
      },
    },
    controls: {
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },

    a11y: {
      // 'todo' - show a11y violations in the test UI only
      // 'error' - fail CI on a11y violations
      // 'off' - skip a11y checks entirely
      test: 'todo',
    },
  },
};

export default preview;
