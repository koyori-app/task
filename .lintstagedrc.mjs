import path from 'node:path';

const FRONTEND = path.resolve('apps/frontend');

export default {
  // 整形のあと、変えたファイルに関係する単体テストだけを回す（全件は pre-push で回す）。
  // lint-staged はコマンドをシェルを通さずに分けるので、bash -c と引用符の入れ子は使わない
  // （入れ子にするとパスが届かず、--passWithNoTests で何も回らずに通る）
  "apps/frontend/**/*.{ts,tsx,vue,js,jsx,mjs,cjs}": (files) => [
    "bash -c \"cd apps/frontend && pnpm fmt\"",
    `pnpm --dir apps/frontend exec vp test related --project unit --run --passWithNoTests ${files
      .map((file) => path.relative(FRONTEND, file))
      .join(" ")}`,
  ],
  "apps/backend/**/*.rs": () => [
    "bash -c \"cd apps/backend && cargo fmt --all\"",
    "bash -c \"cd apps/backend && cargo clippy --workspace --all-targets -- -D warnings\"",
  ],
};
