#!/usr/bin/env node
// docs/STYLE.md の規則のうち機械的に見られるものを検査する。依存なし。
// 使い方: node docs/scripts/lint-style.mjs [files...]   引数が無ければ docs/ の公開ページ全部
// 出力:   path:line: error|warn rule: message          error が 1 件でもあれば exit 1
// scripts/lint-allowlist.txt に載せたページ（移行前）は error を warn に落とす

import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const DOCS = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PUBLISHED_DIRS = ["features", "frontend", "development", "getting-started", "guide", "self-hosting", "integrations", "dev"];
// 公開ページに加えて、規則と雛形も自分の規則で検査する
const PUBLISHED_ROOT_FILES = ["index.md", "breadcrumbs.md", "STYLE.md", "_template-spec.md", "_template-guide.md"];
const STATUS = new Set(["implemented", "partial", "planned", "operational"]);
const SPEC_SECTIONS = ["概要", "用語", "振る舞い", "データ", "API", "認可", "画面", "未実装と既知の制限", "決定の記録"];
const BANNED = [
  [/札(?=[はをがにのへと、。）\s]|$)/u, "札 → メンションのチップ"],
  [/印(?=[はをがにのへと、。）\s]|$)/u, "印 → メンションのマーカー"],
  [/控え/u, "控え → 保存した値"],
  [/正本/u, "正本 → 定義元"],
  [/防波堤/u, "防波堤 → 最後の制約"],
  [/権威/u, "権威 → 判定の元"],
  [/客分/u, "客分 → ゲスト"],
  [/地雷/u, "地雷 → 既知の落とし穴"],
  [/沼/u, "沼 → 何が起きるかを書く"],
  [/(?<![入出窓間])口(?=[はをがにへの、。）]|$)/u, "口 → エンドポイント"],
  [/(?<![国])旗/u, "旗 → フラグ"],
];

const allowlistPath = join(DOCS, "scripts", "lint-allowlist.txt");
const allowlist = new Set(
  existsSync(allowlistPath)
    ? readFileSync(allowlistPath, "utf8").split("\n").map((l) => l.trim()).filter((l) => l && !l.startsWith("#"))
    : [],
);

function walk(dir, out) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (name.endsWith(".md")) out.push(p);
  }
  return out;
}

function targets(argv) {
  if (argv.length) return argv.map((a) => resolve(a));
  const files = PUBLISHED_ROOT_FILES.map((f) => join(DOCS, f)).filter(existsSync);
  for (const d of PUBLISHED_DIRS) if (existsSync(join(DOCS, d))) walk(join(DOCS, d), files);
  return files;
}

function parseFrontmatter(lines) {
  if (lines[0] !== "---") return { fm: null, end: -1 };
  const end = lines.indexOf("---", 1);
  if (end < 0) return { fm: null, end: -1 };
  const fm = {};
  let section = null;
  for (const raw of lines.slice(1, end)) {
    const m = raw.match(/^(\s*)([A-Za-z_][\w-]*):\s*(.*)$/);
    if (!m) continue;
    const [, indent, key, value] = m;
    if (indent.length === 0) {
      section = value === "" ? key : null;
      fm[key] = value.replace(/\s+#.*$/, "").replace(/^["']|["']$/g, "");
    } else if (section) {
      fm[`${section}.${key}`] = value.replace(/\s+#.*$/, "");
    }
  }
  return { fm, end };
}

function lint(file) {
  const rel = relative(DOCS, file).split("\\").join("/");
  const text = readFileSync(file, "utf8");
  const lines = text.split("\n");
  const findings = [];
  const isSpec = (/^(dev\/)?features\/.+\.md$/.test(rel) && !rel.endsWith("/index.md")) || rel === "_template-spec.md";
  const relaxed = allowlist.has(rel);
  // `<!-- lint-disable: rule1, rule2 -->` の行以降、そのファイルではその規則を見ない（STYLE.md の言い換え表などに使う）
  const disabled = new Set();
  const add = (line, level, rule, message) => {
    if (disabled.has(rule)) return;
    if (level === "error" && relaxed) level = "warn";
    findings.push({ file: rel, line, level, rule, message });
  };

  const { fm, end } = parseFrontmatter(lines);
  if (!fm) {
    add(1, "error", "frontmatter", "frontmatter（--- で囲んだ title などの欄）が無い");
  } else {
    for (const k of ["title", "description", "updated"]) if (!fm[k]) add(1, "error", "frontmatter", `${k} が無い`);
    if (fm.updated && !/^\d{4}-\d{2}-\d{2}$/.test(fm.updated)) add(1, "error", "frontmatter", "updated は YYYY-MM-DD で書く");
    if (fm.title && /(仕様書|設計書|実装ノート|（フォローアップ）)/.test(fm.title)) add(1, "error", "title", "title に「仕様書」「設計書」などの種別語を付けない");
    if (isSpec) {
      if (!fm.status) add(1, "error", "frontmatter", "仕様のページには status が要る");
      else if (!STATUS.has(fm.status)) add(1, "error", "frontmatter", `status は ${[...STATUS].join(" / ")} のいずれか`);
    } else if (fm.status) {
      add(1, "error", "frontmatter", "status は仕様（dev/features/）のページだけに付ける");
    }
  }

  let inCode = false;
  let sectionIdx = -1;
  let boldInSection = 0;
  let sectionLine = 0;
  const flushBold = () => {
    if (boldInSection >= 3) add(sectionLine, "warn", "bold", `1 節に太字が ${boldInSection} 箇所ある（2 箇所まで）`);
    boldInSection = 0;
  };

  for (let i = end + 1; i < lines.length; i++) {
    const line = lines[i];
    const no = i + 1;
    if (/^\s*(```|~~~)/.test(line)) {
      inCode = !inCode;
      continue;
    }
    if (inCode) continue;
    const directive = line.match(/^<!--\s*lint-disable:\s*([\w, -]+)\s*-->$/);
    if (directive) {
      for (const r of directive[1].split(",")) disabled.add(r.trim());
      continue;
    }
    const prose = line.replace(/`[^`]*`/g, "");

    if (/^---\s*$/.test(line)) add(no, "error", "hr", "節の区切りの --- を置かない");

    const h = line.match(/^(#{1,6})\s+(.*)$/);
    if (h) {
      const [, hashes, title] = h;
      if (hashes.length === 1) add(no, "error", "heading", "本文に # 見出しを書かない（title が H1 になる）");
      if (/^\d+(\.\d+)*\.?\s/.test(title)) add(no, "error", "heading", "見出しに番号を付けない");
      if (hashes.length === 2) {
        flushBold();
        sectionLine = no;
        if (isSpec) {
          const name = title.replace(/\s*\(.*\)$/, "").trim();
          const idx = SPEC_SECTIONS.indexOf(name);
          if (idx < 0) add(no, "error", "spec-section", `仕様の節は ${SPEC_SECTIONS.join(" / ")} のいずれかにする`);
          else if (idx < sectionIdx) add(no, "error", "spec-section", "仕様の節の順が STYLE.md と違う");
          else sectionIdx = idx;
        }
      }
      continue;
    }

    if (/[:：]\s*$/.test(line) && !/^\s*\|/.test(line)) add(no, "error", "trailing-colon", "行末のコロンを書かず、文で終える");
    if (/[—―]/.test(prose)) add(no, "error", "dash", "ダッシュ（—）を本文に書かない");
    if (/§\s*\d/.test(prose) || /(?<![\w.])\d+(\.\d+)?\s*節(?![目])/.test(prose)) add(no, "error", "section-ref", "§N や「N 節」ではなく見出しの語とリンクで参照する");
    if (/ユーザー(?!名|エージェント|アクセストークン|認可)/.test(prose)) add(no, "warn", "term", "「利用者」に統一する（UI 文言の引用は除く）");
    for (const [re, msg] of BANNED) if (re.test(prose)) add(no, "warn", "term", `比喩・造語: ${msg}`);
    boldInSection += (line.match(/\*\*[^*]+\*\*/g) ?? []).length;
  }
  flushBold();
  return findings;
}

const files = targets(process.argv.slice(2));
const verbose = process.argv.includes("--verbose");
let errors = 0;
let warns = 0;
let pending = 0;
for (const f of files) {
  const rel = relative(DOCS, f).split("\\").join("/");
  const found = lint(f);
  // 移行前のページは 1 行にまとめる（全部出すと CI のログが千行を超える）。--verbose で全部出す
  if (allowlist.has(rel) && !verbose) {
    pending++;
    console.log(`${rel}: ${found.length} 件（未移行。scripts/lint-allowlist.txt に載っている）`);
    continue;
  }
  for (const x of found) {
    if (x.level === "error") errors++;
    else warns++;
    console.log(`${x.file}:${x.line}: ${x.level} ${x.rule}: ${x.message}`);
  }
}
console.log(`\n${files.length} files, ${errors} errors, ${warns} warnings, ${pending} pending`);
process.exit(errors ? 1 : 0);
