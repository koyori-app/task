import { runInThisContext } from 'node:vm';
import { themeScript } from '@koyori-app/ui-vue';
import { createHead } from '@unhead/vue/server';
import type { PageContextServer } from 'vike/types';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createSSRApp } from 'vue';
import { renderToString } from 'vue/server-renderer';

vi.mock('vike-vue/usePageContext', () => ({
  usePageContext: () => ({ settings: { env: {} } }),
}));

import Head from '../+Head.vue';
import { headHtmlEnd } from '../+headHtmlEnd';

async function renderHeadHtml(): Promise<string> {
  const head = createHead({ disableDefaults: true });
  const app = createSSRApp(Head);
  app.use(head);
  await renderToString(app);
  return headHtmlEnd({ _unhead: head } as unknown as PageContextServer);
}

function themeScriptIn(html: string): string | undefined {
  return [...html.matchAll(/<script[^>]*>([\s\S]*?)<\/script>/g)]
    .map((match) => match[1])
    .find((body) => body.includes('koyori-theme'));
}

function stubOs(scheme: 'light' | 'dark') {
  const listeners: Array<() => void> = [];
  const media = {
    matches: scheme === 'light',
    addEventListener: (_: string, listener: () => void) => listeners.push(listener),
  };
  vi.stubGlobal('matchMedia', (query: string) => {
    if (query !== '(prefers-color-scheme: light)') throw new Error(`知らぬ問い: ${query}`);
    return media;
  });
  return {
    change(next: 'light' | 'dark') {
      media.matches = next === 'light';
      for (const listener of listeners) listener();
    },
  };
}

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});
afterEach(() => vi.unstubAllGlobals());

describe('head の themeScript（html の data-theme を描画の前に決める）', () => {
  it('head の HTML に ui の themeScript が inline でそのまま入る', async () => {
    const html = await renderHeadHtml();

    expect(themeScriptIn(html)).toBe(themeScript);
  });

  it.each(['light', 'dark'] as const)(
    'まだ選んでいなければ、OS の配色（%s）に合わせる',
    async (scheme) => {
      stubOs(scheme);
      const script = themeScriptIn(await renderHeadHtml());

      runInThisContext(script ?? '');

      expect(document.documentElement.dataset.theme).toBe(scheme);
    },
  );

  it.each([
    ['light', 'dark'],
    ['dark', 'light'],
  ] as const)(
    '選んだ配色（%s）は、OS が逆（%s）でも、後から OS が変わっても保つ',
    async (chosen, os) => {
      const stub = stubOs(os);
      localStorage.setItem('koyori-theme', chosen);
      const script = themeScriptIn(await renderHeadHtml());

      runInThisContext(script ?? '');
      expect(document.documentElement.dataset.theme).toBe(chosen);

      stub.change(chosen);
      stub.change(os);
      expect(document.documentElement.dataset.theme).toBe(chosen);
    },
  );

  it('選んでいない間は、開いたまま OS の配色が変わると追従する', async () => {
    const stub = stubOs('dark');
    const script = themeScriptIn(await renderHeadHtml());

    runInThisContext(script ?? '');
    expect(document.documentElement.dataset.theme).toBe('dark');

    stub.change('light');
    expect(document.documentElement.dataset.theme).toBe('light');
    stub.change('dark');
    expect(document.documentElement.dataset.theme).toBe('dark');
  });
});
