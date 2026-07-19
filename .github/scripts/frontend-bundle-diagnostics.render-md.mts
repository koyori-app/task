/*
 * SPDX-FileCopyrightText: syuilo and misskey-project
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * Requires Node.js 24+ (.mts direct execution and node:zlib zstdCompressSync in vite.config.ts).
 *
 * Report body (frontend-bundle-diagnostics-report.md) is build output from the PR checkout;
 * treat its contents as untrusted when posting comments or reviewing.
 * レポート本文(frontend-bundle-diagnostics-report.md)は PR 側のビルド出力であり内容は信頼できない。
 */

import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as utility from './frontend-bundle-diagnostics.utility.mts';

export type Metric = {
  rendered: number;
  gzip: number;
  brotli: number;
  zstd: number;
};

export type VisualizerReport = {
  nodeParts?: Record<
    string,
    {
      renderedLength?: number;
      gzipLength?: number;
      brotliLength?: number;
      zstdLength?: number;
    }
  >;
  nodeMetas?: Record<
    string,
    {
      moduleParts?: Record<string, string>;
    }
  >;
};

export const emptyMetric = (): Metric => ({ rendered: 0, gzip: 0, brotli: 0, zstd: 0 });

export function stableChunkName(file: string) {
  return file.replace(/([.-])[A-Za-z0-9_-]{8,}(?=\.(?:m?js|css)$)/, '$1[hash]');
}

export function collectChunks(report: VisualizerReport) {
  const chunks = new Map<string, Metric>();
  for (const meta of Object.values(report.nodeMetas ?? {})) {
    for (const [file, partId] of Object.entries(meta.moduleParts ?? {})) {
      const part = report.nodeParts?.[partId];
      if (!part) continue;
      const key = stableChunkName(file);
      const metric = chunks.get(key) ?? emptyMetric();
      metric.rendered += part.renderedLength ?? 0;
      metric.gzip += part.gzipLength ?? 0;
      metric.brotli += part.brotliLength ?? 0;
      metric.zstd += part.zstdLength ?? 0;
      chunks.set(key, metric);
    }
  }
  return chunks;
}

export function sum(chunks: Map<string, Metric>) {
  const total = emptyMetric();
  for (const metric of chunks.values()) {
    total.rendered += metric.rendered;
    total.gzip += metric.gzip;
    total.brotli += metric.brotli;
    total.zstd += metric.zstd;
  }
  return total;
}

export function getChunkStatus(before: Map<string, Metric>, after: Map<string, Metric>, name: string) {
  if (!before.has(name)) return 'new';
  if (!after.has(name)) return 'removed';
  return 'changed';
}

export function isUnchangedChunk(
  before: Map<string, Metric>,
  after: Map<string, Metric>,
  name: string,
) {
  const status = getChunkStatus(before, after, name);
  if (status !== 'changed') return false;
  const beforeMetric = before.get(name) ?? emptyMetric();
  const afterMetric = after.get(name) ?? emptyMetric();
  return (
    beforeMetric.rendered === afterMetric.rendered &&
    beforeMetric.gzip === afterMetric.gzip &&
    beforeMetric.brotli === afterMetric.brotli &&
    beforeMetric.zstd === afterMetric.zstd
  );
}

export function metricCells(before: Metric, after: Metric, field: keyof Metric) {
  return [
    utility.formatBytes(before[field]),
    utility.formatBytes(after[field]),
    utility.formatDelta(after[field] - before[field]),
    utility.formatPercent(before[field], after[field]),
  ];
}

export const compressionFootnote =
  '> zstd (and gzip/brotli) are per-part compression sums, not one-shot compressed sizes for whole chunks. ' +
  'Suitable for before/after deltas; absolute values overstate real delivery size.';

export const baseBootstrapNote = '> この差分は依存変更を反映していない';

async function renderReport(beforeFile: string, afterFile: string, outputFile: string) {
  const before = collectChunks(JSON.parse(await fs.readFile(beforeFile, 'utf8')) as VisualizerReport);
  const after = collectChunks(JSON.parse(await fs.readFile(afterFile, 'utf8')) as VisualizerReport);
  const beforeTotal = sum(before);
  const afterTotal = sum(after);
  const names = [...new Set([...before.keys(), ...after.keys()])];
  names.sort((a, b) => {
    const aDelta = Math.abs((after.get(a)?.rendered ?? 0) - (before.get(a)?.rendered ?? 0));
    const bDelta = Math.abs((after.get(b)?.rendered ?? 0) - (before.get(b)?.rendered ?? 0));
    return bDelta - aDelta || a.localeCompare(b);
  });

  const lines = ['## 📦 Frontend bundle diagnostics', ''];
  if (process.env.FRONTEND_BUNDLE_BASE_BOOTSTRAPPED) {
    lines.push(baseBootstrapNote, '');
  }
  lines.push(
    '| Metric | Before | After | Δ | Δ (%) |',
    '| --- | ---: | ---: | ---: | ---: |',
    `| Raw | ${metricCells(beforeTotal, afterTotal, 'rendered').join(' | ')} |`,
    `| Gzip | ${metricCells(beforeTotal, afterTotal, 'gzip').join(' | ')} |`,
    `| Brotli | ${metricCells(beforeTotal, afterTotal, 'brotli').join(' | ')} |`,
    `| Zstd | ${metricCells(beforeTotal, afterTotal, 'zstd').join(' | ')} |`,
    '',
    compressionFootnote,
    '',
    '### Chunk changes',
    '',
    '| Chunk | Status | Raw Δ | Raw Δ (%) | Gzip Δ | Gzip Δ (%) | Brotli Δ | Brotli Δ (%) | Zstd Δ | Zstd Δ (%) |',
    '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |',
  );

  let changedChunkCount = 0;
  for (const name of names) {
    const beforeMetric = before.get(name) ?? emptyMetric();
    const afterMetric = after.get(name) ?? emptyMetric();
    const status = getChunkStatus(before, after, name);
    if (isUnchangedChunk(before, after, name)) continue;
    changedChunkCount += 1;
    lines.push(
      `| \`${utility.escapeTableCell(name)}\` | ${status} | ${utility.formatDelta(afterMetric.rendered - beforeMetric.rendered)} | ${utility.formatPercent(beforeMetric.rendered, afterMetric.rendered)} | ${utility.formatDelta(afterMetric.gzip - beforeMetric.gzip)} | ${utility.formatPercent(beforeMetric.gzip, afterMetric.gzip)} | ${utility.formatDelta(afterMetric.brotli - beforeMetric.brotli)} | ${utility.formatPercent(beforeMetric.brotli, afterMetric.brotli)} | ${utility.formatDelta(afterMetric.zstd - beforeMetric.zstd)} | ${utility.formatPercent(beforeMetric.zstd, afterMetric.zstd)} |`,
    );
  }

  if (changedChunkCount === 0)
    lines.push('| _No changed chunks_ | — | — | — | — | — | — | — | — | — |');
  if (process.env.FRONTEND_BUNDLE_REPORT_ARTIFACT_URL) {
    lines.push('', `[Open interactive treemap](${process.env.FRONTEND_BUNDLE_REPORT_ARTIFACT_URL})`);
  }

  await fs.writeFile(outputFile, `${lines.join('\n')}\n`);
}

const isMain =
  process.argv[1] != null &&
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  const [beforeFile, afterFile, outputFile] = process.argv.slice(2);
  if (!beforeFile || !afterFile || !outputFile) {
    throw new Error('Usage: render-md.mts <before-stats.json> <after-stats.json> <report.md>');
  }
  await renderReport(beforeFile, afterFile, outputFile);
}
