import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor } from 'storybook/test';
import brokenHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-broken.html?raw';
import c4Html from '@/lib/kfm-story-fixtures/rendered/mermaid-c4.html?raw';
import clickHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-click.html?raw';
import flowchartHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-flowchart.html?raw';
import noneHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-none.html?raw';
import sequenceHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-sequence.html?raw';
import stateHtml from '@/lib/kfm-story-fixtures/rendered/mermaid-state.html?raw';
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
// CSS サイドカー: レンダラは CSS を import しない契約のため、消費側 (= story) が明示 import
// (器 .kfm-content のスタイルは GFM サイドカー側 — 他 story と同じ二枚組＋mermaid 独自分)
import '@/lib/remark-gfm/style.css';
import '@/lib/rehype-starry-night/style.css';
import '@/lib/rehype-kfm-code/style.css';
import '@/lib/remark-kfm-mermaid/style.css';
// 本番では +client.ts が行う client 登録を story でも同じ入口で行う (二重 define は registry 側で防止)
import { registerKfmCustomElements } from '@/lib/markup-renderer/_client-registry';
import { htmlTheme } from '../html-theme';

registerKfmCustomElements();

/*
 * KFM mermaid 図の story 群。fixture は renderDescription の事前生成 HTML だが、
 * 中身は SSR がブラウザへ渡す不活性 <kfm-mermaid> ＋ソーステキストのみで SVG を含まない
 * (mermaid は本質的にブラウザ描画のため、SVG まで事前 HTML 化する方式は採らない)。
 * 図の描画は story 上で custom element が実行し、play は data-kfm-mermaid の状態を待つ
 * (時間待ちなし)。この待ちが VRT の「描画完了してから撮る」口を兼ねる。
 */

type KfmStoryArgs = { html: string };

const kfmRender = (args: KfmStoryArgs) => ({
  setup: () => ({ args }),
  template: `<div class="${KFM_CONTENT_CLASS}" v-html="args.html" />`,
});

const meta = {
  title: 'KFM/Mermaid',
  tags: ['autodocs'],
  render: kfmRender,
  // KFM のストーリーは明るい配色を既定にし、暗い配色のものだけ上書きする
  beforeEach: htmlTheme('light'),
} satisfies Meta<KfmStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 描画完了 (成功) を状態で待ち、shadow DOM に SVG が実在することまで確かめる共通 assert */
async function expectRendered(root: ParentNode, selector = 'kfm-mermaid'): Promise<HTMLElement> {
  const element = root.querySelector<HTMLElement>(selector);
  await expect(element).not.toBeNull();
  await waitFor(
    () => expect(element?.dataset.kfmMermaid, 'data-kfm-mermaid が立つまで待つ').toBe('rendered'),
    { timeout: 10_000 },
  );
  await expect(element?.shadowRoot?.querySelector('svg')).not.toBeNull();
  return element as HTMLElement;
}

export const Flowchart: Story = {
  name: 'Flowchart（基本）',
  args: { html: flowchartHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: 図が出ずソーステキストのままなら client 登録か remark 層の変換の崩れ。data-kfm-mermaid が立たなければ描画完了シグナルの崩れ (VRT の待ち口が死ぬ)。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expectRendered(canvasElement);
  },
};

export const Sequence: Story = {
  name: 'Sequence（シーケンス図）',
  args: { html: sequenceHtml },
  parameters: {
    docs: {
      description: {
        story: '壊れたら: participant/矢印の並びが崩れたら mermaid 更新か描画経路の崩れ。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expectRendered(canvasElement);
  },
};

export const State: Story = {
  name: 'State（状態遷移図）',
  args: { html: stateHtml },
  parameters: {
    docs: {
      description: {
        story: '壊れたら: [*] 始端/終端や遷移ラベルが消えたら mermaid 更新か描画経路の崩れ。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expectRendered(canvasElement);
  },
};

export const C4Context: Story = {
  name: 'C4（Person が絵として出る・正常出力の回帰）',
  args: { html: c4Html },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: C4 の Person が絵として出なくなった崩れ。mermaid 11 は <image xlink:href="data:image/png;base64,…">（挿入前検査 (element.ts) が data: を画像 sink まで許すことの回帰アンカーを兼ねる）、12 は unified renderer の vector 形（g.c4-person の頭の circle ＋ 胴の rect）で描く。どちらの姿も無ければ、renderer の崩れか挿入前検査の退行。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const element = await expectRendered(canvasElement);
    // 「Person が絵として出る」ことの釘。版で姿が違う:
    //   mermaid 11: <image xlink:href="data:image/png;base64,…">（PNG 埋め込み）
    //   mermaid 12: <g class="… c4-person"> の中の <circle>（頭）＋ rect（胴）の vector 形
    // どちらか一方が実在すれば絵は出ておる。単なる null 裏返しにせぬ——
    // 見つけた方の中身（data: 接頭辞 / person 形の頭）まで検める
    const shadow = element.shadowRoot;
    const icon = shadow?.querySelector('image') ?? null;
    const person = shadow?.querySelector('g.c4-person') ?? null;
    const personHead = person?.querySelector('circle') ?? null;
    const personBody = person?.querySelector('rect') ?? null;
    await expect(icon !== null || (personHead !== null && personBody !== null)).toBe(true);
    if (icon) {
      const href = icon.getAttribute('xlink:href') ?? icon.getAttribute('href') ?? '';
      await expect(href.startsWith('data:image/')).toBe(true);
    }
  },
};

export const ClickLink: Story = {
  name: 'Click（リンク付き flowchart・正常出力の回帰）',
  args: { html: clickHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: click 付き flowchart は mermaid の正常出力であり rendered になるべき。error へ倒れたら、挿入前検査 (element.ts) が sink (shadow への HTML パース) と違う文法で正常出力を偽陽性にしている崩れ——実 mermaid 出力を通すこの story がその回帰アンカー。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expectRendered(canvasElement);
  },
};

export const BrokenInput: Story = {
  name: '記法が壊れている入力（error 状態）',
  args: { html: brokenHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: エラー図や空白が出たら fail 側の契約 (shadow を張らずソーステキスト残置 ＋ data-kfm-mermaid="error") の崩れ。rendered になったら mermaid が構文エラーを黙って通した変化。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const element = canvasElement.querySelector<HTMLElement>('kfm-mermaid');
    await expect(element).not.toBeNull();
    await waitFor(
      () => expect(element?.dataset.kfmMermaid, 'error 状態へ倒れるまで待つ').toBe('error'),
      { timeout: 10_000 },
    );
    // フォールバック: shadow を張らず、ソーステキストがそのまま見える
    await expect(element?.shadowRoot).toBeNull();
    await expect(element?.textContent).toContain('閉じ括弧がない');
    await expect(getComputedStyle(element as HTMLElement).whiteSpace).toBe('pre');
    await expect(getComputedStyle(element as HTMLElement).overflowX).toBe('auto');
  },
};

export const DarkTheme: Story = {
  name: 'ダークテーマ',
  args: { html: flowchartHtml },
  // html の data-theme = dark（アプリ本体と同方式）。明るい配色との違いは play が実ブラウザで比べる
  beforeEach: htmlTheme('dark'),
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: html の data-theme からのテーマ切替 (element.ts) の喪失——alerts/着色の「ライト固定」と同型の再発をこの絵で検知する。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expect(document.documentElement.dataset.theme).toBe('dark');
    const dark = await expectRendered(canvasElement, 'kfm-mermaid');
    // 同じソースを html の data-theme = light で描き直し、SVG が暗い配色のときと違うことを確かめる。
    // 描画ごとに変わる SVG の id（kfm-mermaid-N）は比べる前に揃える。
    // theme は htmlTheme の後始末が元に戻すが、ここでも dark に戻して他のストーリーへ漏らさない
    const root = document.documentElement;
    const light = document.createElement('kfm-mermaid');
    light.textContent = Array.from(dark.childNodes)
      .filter((node): node is Text => node.nodeType === Node.TEXT_NODE)
      .map((node) => node.data)
      .join('');
    root.dataset.theme = 'light';
    try {
      canvasElement.append(light);
      await waitFor(
        () => expect(light.dataset.kfmMermaid, '明るい配色の描画が終わるまで待つ').toBe('rendered'),
        { timeout: 10_000 },
      );
      const normalize = (html: string | undefined) =>
        html?.replace(/kfm-mermaid-\d+/g, 'kfm-mermaid-N');
      const darkSvg = normalize(dark.shadowRoot?.innerHTML);
      const lightSvg = normalize(light.shadowRoot?.innerHTML);
      await expect(darkSvg).toContain('<svg');
      await expect(lightSvg).toContain('<svg');
      await expect(lightSvg).not.toBe(darkSvg);
    } finally {
      light.remove();
      root.dataset.theme = 'dark';
    }
  },
};

export const NoDiagram: Story = {
  name: '図を含まないページ（何も起きない）',
  args: { html: noneHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: 図なしページに kfm-mermaid が生まれたら remark 層の捕捉範囲の崩れ。ts フェンスが pre>code でなくなったら他フェンスへの越境。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector('kfm-mermaid')).toBeNull();
    // 陽性対照: 通常のコードフェンスは無傷で pre>code のまま
    await expect(canvasElement.querySelector('pre code')).not.toBeNull();
  },
};
