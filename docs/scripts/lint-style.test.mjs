import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const script = join(dirname(fileURLToPath(import.meta.url)), "lint-style.mjs");

function lint(markdown) {
  const dir = mkdtempSync(join(tmpdir(), "lint-style-"));
  const file = join(dir, "page.md");
  writeFileSync(file, markdown);
  const run = spawnSync(process.execPath, [script, file], { encoding: "utf8" });
  return { status: run.status, out: run.stdout };
}

const frontmatter = `---
title: 例
description: フェンスの検査
updated: 2026-11-05
---
`;

test("```` で囲んだ中の ``` はコードの中身として扱い、本文として検査しない", () => {
  const { status, out } = lint(`${frontmatter}
## 書き方

次のように書く。

\`\`\`\`md
---
title: 例
---

## 1. 番号付きの見出し

\`\`\`ts
const key: string = "値:";
\`\`\`

本文の続き:
\`\`\`\`

この行は検査される。
`);
  assert.equal(status, 0, out);
  assert.match(out, /0 errors/);
});

test("~~~ で開いたフェンスは ``` では閉じない", () => {
  const { status, out } = lint(`${frontmatter}
## 書き方

~~~
\`\`\`
## 1. 番号付きの見出し
~~~
`);
  assert.equal(status, 0, out);
});

test("``` の中の ```ts は後ろに文字があるので閉じフェンスにしない", () => {
  const { status, out } = lint(`${frontmatter}
## 書き方

\`\`\`
\`\`\`ts
key:
\`\`\`

本文の続き。
`);
  assert.equal(status, 0, out);
});

test("フェンスの外の規則違反はこれまでどおり error になる", () => {
  const { status, out } = lint(`${frontmatter}
## 1. 番号付きの見出し

本文:
`);
  assert.equal(status, 1);
  assert.match(out, /heading/);
  assert.match(out, /trailing-colon/);
});
