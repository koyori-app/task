// usage: node scripts/collect-runtime-deps.mjs <出力先の node_modules> <パッケージ名>...
// pnpm が lockfile どおりに入れた node_modules から、指定パッケージとその依存の閉包だけを写す
// （apps/frontend/Dockerfile が配布イメージ用に使う）。版を解き直さないので、CI と同じ版になる。
// pnpm の仮想ストア（node_modules/.pnpm/<id>/node_modules/<名>、依存は隣の相対リンク）の形を
// そのまま保つので、リンクは写した先でも通る。
import fs from 'node:fs';
import path from 'node:path';

const [dest, ...names] = process.argv.slice(2);
if (!dest || names.length === 0)
  throw new Error('usage: collect-runtime-deps.mjs <dest> <name>...');

const MARK = `${path.sep}node_modules${path.sep}.pnpm${path.sep}`;
const seen = new Set();

/** パッケージの実体から仮想ストアの id を取り、その塊を写して依存へ進む。id を返す */
function visit(link) {
  const real = fs.realpathSync(link);
  const at = real.lastIndexOf(MARK);
  if (at < 0) throw new Error(`pnpm の仮想ストアの外を指している: ${link} -> ${real}`);
  const id = real.slice(at + MARK.length).split(path.sep)[0];
  if (seen.has(id)) return id;
  seen.add(id);
  const src = real.slice(0, at + MARK.length) + id;
  fs.cpSync(src, path.join(dest, '.pnpm', id), { recursive: true, verbatimSymlinks: true });
  const siblings = path.join(src, 'node_modules');
  for (const entry of fs.readdirSync(siblings, { withFileTypes: true })) {
    const full = path.join(siblings, entry.name);
    if (entry.isSymbolicLink()) visit(full);
    else if (entry.name.startsWith('@')) {
      for (const scoped of fs.readdirSync(full, { withFileTypes: true })) {
        if (scoped.isSymbolicLink()) visit(path.join(full, scoped.name));
      }
    }
  }
  return id;
}

for (const name of names) {
  const id = visit(path.join('node_modules', name));
  const target = path.join(dest, name);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.symlinkSync(
    path.relative(path.dirname(target), path.join(dest, '.pnpm', id, 'node_modules', name)),
    target,
  );
}
console.log(`runtime deps: ${names.join(', ')} -> ${seen.size} packages`);
