export default {
  "apps/frontend/**/*.{ts,tsx,vue,js,jsx,mjs,cjs}": () =>
    "bash -c \"cd apps/frontend && pnpm fmt\"",
  "apps/backend/**/*.rs": () =>
    "bash -c \"cd apps/backend && cargo fmt --all\"",
};
