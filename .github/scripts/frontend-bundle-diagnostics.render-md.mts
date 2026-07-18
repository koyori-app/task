/*
 * SPDX-FileCopyrightText: syuilo and misskey-project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

import { promises as fs } from 'node:fs';
import * as utility from './frontend-bundle-diagnostics.utility.mts';

type Metric = {
  rendered: number;
  gzip: number;
  brotli: number;
};

type VisualizerReport = {
  nodeParts?: Record<string, {
    renderedLength?: number;
    gzipLength?: number;
    brotliLength?: number;
  }>;
  nodeMetas?: Record<string, {
    moduleParts?: Record<string, string>;
  }>;
};

const emptyMetric = (): Metric => ({ rendered: 0, gzip: 0, brotli: 0 });

function stableChunkName(file: string) {
  return file.replace(/([.-])[A-Za-z0-9_-]{8,}(?=\.(?:m?js|css)$)/, '$1[hash]');
}

function collectChunks(report: VisualizerReport) {
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
      chunks.set(key, metric);
    }
  }
  return chunks;
}

function sum(chunks: Map<string, Metric>) {
  const total = emptyMetric();
  for (const metric of chunks.values()) {
    total.rendered += metric.rendered;
    total.gzip += metric.gzip;
    total.brotli += metric.brotli;
  }
  return total;
}

function metricCells(before: Metric, after: Metric, field: keyof Metric) {
  return [
    utility.formatBytes(before[field]),
    utility.formatBytes(after[field]),
    utility.formatDelta(after[field] - before[field]),
    utility.formatPercent(before[field], after[field]),
  ];
}

const [beforeFile, afterFile, outputFile] = process.argv.slice(2);
if (!beforeFile || !afterFile || !outputFile) {
  throw new Error('Usage: render-md.mts <before-stats.json> <after-stats.json> <report.md>');
}

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

const lines = [
  '## 📦 Frontend bundle diagnostics',
  '',
  '| Metric | Before | After | Δ | Δ (%) |',
  '| --- | ---: | ---: | ---: | ---: |',
  `| Raw | ${metricCells(beforeTotal, afterTotal, 'rendered').join(' | ')} |`,
  `| Gzip | ${metricCells(beforeTotal, afterTotal, 'gzip').join(' | ')} |`,
  `| Brotli | ${metricCells(beforeTotal, afterTotal, 'brotli').join(' | ')} |`,
  '',
  '### Chunk changes',
  '',
  '| Chunk | Status | Raw Δ | Raw Δ (%) | Gzip Δ | Gzip Δ (%) | Brotli Δ | Brotli Δ (%) |',
  '| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |',
];

let changedChunkCount = 0;
for (const name of names) {
  const beforeMetric = before.get(name) ?? emptyMetric();
  const afterMetric = after.get(name) ?? emptyMetric();
  const status = !before.has(name) ? 'new' : !after.has(name) ? 'removed' : 'changed';
  if (status === 'changed' && beforeMetric.rendered === afterMetric.rendered && beforeMetric.gzip === afterMetric.gzip && beforeMetric.brotli === afterMetric.brotli) continue;
  changedChunkCount += 1;
  lines.push(`| \`${utility.escapeTableCell(name)}\` | ${status} | ${utility.formatDelta(afterMetric.rendered - beforeMetric.rendered)} | ${utility.formatPercent(beforeMetric.rendered, afterMetric.rendered)} | ${utility.formatDelta(afterMetric.gzip - beforeMetric.gzip)} | ${utility.formatPercent(beforeMetric.gzip, afterMetric.gzip)} | ${utility.formatDelta(afterMetric.brotli - beforeMetric.brotli)} | ${utility.formatPercent(beforeMetric.brotli, afterMetric.brotli)} |`);
}

if (changedChunkCount === 0) lines.push('| _No changed chunks_ | — | — | — | — | — | — | — |');
if (process.env.FRONTEND_BUNDLE_REPORT_ARTIFACT_URL) {
  lines.push('', `[Open interactive treemap](${process.env.FRONTEND_BUNDLE_REPORT_ARTIFACT_URL})`);
}

await fs.writeFile(outputFile, `${lines.join('\n')}\n`);
