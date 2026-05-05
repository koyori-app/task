import { defineConfig } from 'vite-plus';

export default defineConfig({
  fmt: {
    singleQuote: true,
    trailingComma: 'all',
    ignorePatterns: ['content/**/*.md']
  },
  lint: { options: { typeAware: true, typeCheck: true } },
  test: {
    maxWorkers: 4,
  },
});
