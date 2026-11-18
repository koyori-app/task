import type { Meta, StoryObj } from '@storybook/vue3-vite';
import { expect, waitFor } from 'storybook/test';
import embeddedHtml from '@/lib/kfm-story-fixtures/rendered/drive-media-embedded.html?raw';
import unavailableHtml from '@/lib/kfm-story-fixtures/rendered/drive-media-unavailable.html?raw';
// CSS サイドカー: レンダラは CSS を import しない契約のため、消費側 (= story) が明示 import
import '@/lib/remark-gfm/style.css';
import { KFM_CONTENT_CLASS } from '@/lib/remark-gfm/content-class';
import { registerKfmCustomElements } from '@/lib/markup-renderer/_client-registry';
import { htmlTheme } from '../html-theme';

registerKfmCustomElements();

/*
 * 本文に貼り付けた Drive のファイルの story 群 (TASK-137)。fixture は renderDescription の
 * 事前生成 HTML (単一ソース = kfm-story-fixtures/inputs.ts、drift 検査 = kfm-story-fixtures.test.ts)。
 * 読み込みの成否は実ブラウザでしか決まらないので、<kfm-media> がリンクへ替える動きはここで固定する。
 * 読み込める方の URL は Storybook の静的ファイル (.storybook/static/api/v1/drive/files/…/content)。
 */

type KfmStoryArgs = { html: string };

const OK_SRC = '/api/v1/drive/files/0f8fad5b-d9cb-469f-a165-70867728950e/content';
const MISSING_SRC = '/api/v1/drive/files/7c9e6679-7425-40de-944b-e07fc1f90ae7/content';

const meta = {
  title: 'KFM/DriveMedia',
  tags: ['autodocs'],
  render: (args: KfmStoryArgs) => ({
    setup: () => ({ args }),
    template: `<div class="${KFM_CONTENT_CLASS}" v-html="args.html" />`,
  }),
  beforeEach: htmlTheme('light'),
} satisfies Meta<KfmStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Embedded: Story = {
  name: '画像は埋め込み、それ以外はダウンロードのリンク',
  args: { html: embeddedHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: 画像が壊れた絵になる (/api の付け足しが外れた)、画像の横にファイル名のリンクが見える (読み込めたのにリンクへ替えた)。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const host = canvasElement.querySelector<HTMLElement>('kfm-media[data-kind="image"]');
    const img = host?.querySelector('img');
    await expect(img?.getAttribute('src')).toBe(OK_SRC);
    await waitFor(() => expect(img?.complete && img.naturalWidth > 0).toBe(true));
    // 読み込めた画像はそのまま出し、退避用のリンクは隠したまま
    await expect(host?.dataset.kfmMedia).toBeUndefined();
    await expect(img).toBeVisible();
    await expect(host?.querySelector('a')).not.toBeVisible();

    // 画像の記法でないリンクは、ダウンロードのリンクになる
    const link = canvasElement.querySelector<HTMLAnchorElement>('p > a');
    await expect(link?.getAttribute('href')).toBe(OK_SRC);
    await expect(link?.hasAttribute('download')).toBe(true);
    await expect(link?.textContent).toBe('spec.pdf');
  },
};

export const Unavailable: Story = {
  name: '読み込めない画像・動画・音声はダウンロードのリンクに替わる',
  args: { html: unavailableHtml },
  parameters: {
    docs: {
      description: {
        story:
          '壊れたら: 壊れた画像の絵・再生できない空のプレイヤーが残る (<kfm-media> が失敗を拾えていない)。HEIC を表示できないブラウザ、Range 未対応の間の Safari の動画・音声がこの姿になる。',
      },
    },
  },
  play: async ({ canvasElement }) => {
    const hosts = [...canvasElement.querySelectorAll<HTMLElement>('kfm-media')];
    await expect(hosts.map((host) => host.dataset.kind)).toEqual(['image', 'video', 'audio']);
    await waitFor(() =>
      expect(hosts.map((host) => host.dataset.kfmMedia)).toEqual(['failed', 'failed', 'failed']),
    );
    for (const host of hosts) {
      await expect(host.querySelector('img, video, audio')).not.toBeVisible();
      const link = host.querySelector('a');
      await expect(link).toBeVisible();
      await expect(link?.getAttribute('href')).toBe(MISSING_SRC);
      await expect(link?.hasAttribute('download')).toBe(true);
    }
    await expect(hosts.map((host) => host.querySelector('a')?.textContent)).toEqual([
      'IMG_0001.HEIC',
      'clip.mov',
      'memo.caf',
    ]);
  },
};
