import { defineConfig } from 'vite-plus';

export default defineConfig({
  fmt: {
    singleQuote: true,
    trailingComma: 'all',
  },
  lint: { options: { typeAware: true, typeCheck: true } },
  test: {
    maxWorkers: 4,
  },
});
