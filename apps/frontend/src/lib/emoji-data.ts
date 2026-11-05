/**
 * ピッカーと札の読み上げが使う絵文字のデータ（emojibase-data）。
 *
 * JSON は合わせて 1MB 近いので動的 import で読み、初期バンドルに入れない。
 * `compact.json` には `version` が無いので、版で切るために `data.json` を読む（gzip で compact +
 * versions/emoji.json と同じ約 100KB）。肌の色の `skins` は読まない。
 */
import { shallowRef } from 'vue';

/** これより新しい Unicode 版の絵文字は OS で描けないことが多いので出さない */
const MAX_EMOJI_VERSION = 15;

export const EMOJI_CATEGORIES = [
  { key: 'frequent', label: 'よく使う', groups: [] },
  { key: 'people', label: '顔と人', groups: [0, 1] },
  { key: 'nature', label: '自然', groups: [3] },
  { key: 'food', label: '食べ物', groups: [4] },
  { key: 'activities', label: '活動', groups: [6] },
  { key: 'travel', label: '旅行', groups: [5] },
  { key: 'objects', label: '物', groups: [7] },
  { key: 'symbols', label: '記号', groups: [8] },
  { key: 'flags', label: '旗', groups: [9] },
] as const;

export type EmojiCategoryKey = (typeof EMOJI_CATEGORIES)[number]['key'];

export interface EmojiEntry {
  emoji: string;
  hexcode: string;
  label: string;
  /** 1 つ目の shortcode（`:` は含まない） */
  shortcode: string | null;
  /** 検索用に正規化した日本語名・タグ・shortcode */
  searchText: string;
}

export interface EmojiSection {
  key: string;
  label: string;
  category: EmojiCategoryKey;
  entries: EmojiEntry[];
}

export interface EmojiDataset {
  sections: EmojiSection[];
  /** `emojiKey` で引く */
  byEmoji: Map<string, EmojiEntry>;
}

interface RawEmoji {
  emoji: string;
  hexcode: string;
  label: string;
  tags?: string[];
  group?: number;
  order?: number;
  version: number;
}
interface RawMessages {
  groups: { order: number; message: string }[];
}
type RawShortcodes = Record<string, string | string[]>;

/** 異体字セレクタの有無を揃えた比較用の鍵（`❤` と `❤️` を同じ絵文字として扱う） */
export function emojiKey(emoji: string): string {
  return emoji.replaceAll('️', '');
}

function normalizeForSearch(text: string): string {
  return text.normalize('NFKC').toLowerCase();
}

export function buildEmojiDataset(
  raw: readonly RawEmoji[],
  messages: RawMessages,
  shortcodes: RawShortcodes,
): EmojiDataset {
  const byGroup = new Map<number, EmojiEntry[]>();
  const byEmoji = new Map<string, EmojiEntry>();
  const usable = raw
    .filter((item) => item.group !== undefined && item.version <= MAX_EMOJI_VERSION)
    .toSorted((a, b) => (a.order ?? 0) - (b.order ?? 0));
  for (const item of usable) {
    const codes = [shortcodes[item.hexcode] ?? []].flat();
    const entry: EmojiEntry = {
      emoji: item.emoji,
      hexcode: item.hexcode,
      label: item.label,
      shortcode: codes[0] ?? null,
      searchText: normalizeForSearch([item.label, ...(item.tags ?? []), ...codes].join('\n')),
    };
    const group = item.group as number;
    byGroup.set(group, [...(byGroup.get(group) ?? []), entry]);
    byEmoji.set(emojiKey(item.emoji), entry);
  }
  const sections = EMOJI_CATEGORIES.flatMap((category) =>
    category.groups.flatMap((group: number): EmojiSection[] => {
      const entries = byGroup.get(group);
      if (!entries) return [];
      const label = messages.groups.find((item) => item.order === group)?.message ?? category.label;
      return [{ key: `group-${group}`, label, category: category.key, entries }];
    }),
  );
  return { sections, byEmoji };
}

/** 部分一致（大文字小文字を区別しない）。`:thumbs:` のように shortcode を囲む `:` は無視する */
export function searchEmoji(dataset: EmojiDataset, query: string): EmojiEntry[] {
  const needle = normalizeForSearch(query).trim().replace(/^:|:$/g, '');
  if (!needle) return [];
  return dataset.sections.flatMap((section) =>
    section.entries.filter((entry) => entry.searchText.includes(needle)),
  );
}

// ─── よく使う ───

export const RECENT_EMOJI_STORAGE_KEY = 'koyori.reactions.recent';
export const FREQUENT_EMOJI_COUNT = 16;
export const DEFAULT_FREQUENT_EMOJI = [
  '👍',
  '❤️',
  '🎉',
  '😄',
  '👀',
  '🙏',
  '✅',
  '🚀',
  '😂',
  '🔥',
  '💯',
  '🤔',
  '👏',
  '🙌',
  '⭐',
  '😭',
] as const;

/** 使った回数。localStorage が使えない（private window 等）か中身が壊れていれば空 */
export function readEmojiUseCounts(): Record<string, number> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(RECENT_EMOJI_STORAGE_KEY) ?? '{}');
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(([, count]) => typeof count === 'number'),
    );
  } catch {
    return {};
  }
}

export function recordEmojiUse(emoji: string): void {
  const counts = readEmojiUseCounts();
  const key = emojiKey(emoji);
  counts[key] = (counts[key] ?? 0) + 1;
  try {
    localStorage.setItem(RECENT_EMOJI_STORAGE_KEY, JSON.stringify(counts));
  } catch {
    // 保存できなくても選ぶ操作は止めない。並びが既定のままになるだけ
  }
}

/** 回数の多い順。同数は既定の並び、使ったことの無いものは既定の並びで埋める */
export function frequentEmojiEntries(
  dataset: EmojiDataset,
  counts: Record<string, number>,
): EmojiEntry[] {
  const keys = [...new Set([...DEFAULT_FREQUENT_EMOJI.map(emojiKey), ...Object.keys(counts)])];
  return keys
    .toSorted((a, b) => (counts[b] ?? 0) - (counts[a] ?? 0))
    .flatMap((key) => dataset.byEmoji.get(key) ?? [])
    .slice(0, FREQUENT_EMOJI_COUNT);
}

// ─── 読み込み ───

/** 読み込み済みのデータ。読み込み前は null（札は日本語名の代わりに絵文字そのものを読み上げる） */
export const emojiDataset = shallowRef<EmojiDataset | null>(null);

let pending: Promise<EmojiDataset> | null = null;

export function loadEmojiData(): Promise<EmojiDataset> {
  pending ??= Promise.all([
    import('emojibase-data/ja/data.json'),
    import('emojibase-data/ja/messages.json'),
    import('emojibase-data/en/shortcodes/emojibase.json'),
  ])
    .then(([data, messages, shortcodes]) => {
      const dataset = buildEmojiDataset(
        data.default as unknown as RawEmoji[],
        messages.default as unknown as RawMessages,
        shortcodes.default as unknown as RawShortcodes,
      );
      emojiDataset.value = dataset;
      return dataset;
    })
    .catch((error: unknown) => {
      // 次に開いた時に読み直せるよう、失敗を持ち越さない
      pending = null;
      throw error;
    });
  return pending;
}

/**
 * 札やカードの日本語名のための先読み。名前は「あれば使う」ものなので失敗しても絵文字のまま表示を続け、
 * 読み込みの失敗はピッカーを開いた時に画面へ出す。
 */
export function preloadEmojiData(): void {
  loadEmojiData().catch(() => undefined);
}

/** 読み込み済みなら日本語名、まだなら null。`emojiDataset` に追従する */
export function emojiName(emoji: string): string | null {
  return emojiDataset.value?.byEmoji.get(emojiKey(emoji))?.label ?? null;
}
