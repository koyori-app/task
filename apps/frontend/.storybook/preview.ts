import type { Preview } from '@storybook/vue3-vite';
import '@/assets/css/tailwind.css';

// Date.now は意図的に凍結していない(経過時間は performance.now を使う)
const MOCKED_NOW = '2026-07-15T12:00:00+09:00';

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
