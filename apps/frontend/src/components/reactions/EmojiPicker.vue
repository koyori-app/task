<script setup lang="ts">
import { Input, Button, Skeleton } from '@koyori-app/ui-vue';
import {
  Apple,
  Clock,
  Flag,
  Hash,
  Leaf,
  Lightbulb,
  Plane,
  Search,
  Smile,
  Volleyball,
} from '@lucide/vue';
import { computed, nextTick, onMounted, ref, watch, type Component } from 'vue';

import {
  EMOJI_CATEGORIES,
  emojiDataset,
  emojiKey,
  frequentEmojiEntries,
  loadEmojiData,
  readEmojiUseCounts,
  recordEmojiUse,
  searchEmoji,
  type EmojiCategoryKey,
  type EmojiEntry,
} from '@/lib/emoji-data';

const COLUMNS = 8;
/** `content-visibility` の仮の高さに使う（CSS の .cell の高さと見出しの高さ） */
const ROW_HEIGHT_PX = 36;
const HEADING_HEIGHT_PX = 28;

const CATEGORY_ICONS: Record<EmojiCategoryKey, Component> = {
  frequent: Clock,
  people: Smile,
  nature: Leaf,
  food: Apple,
  activities: Volleyball,
  travel: Plane,
  objects: Lightbulb,
  symbols: Hash,
  flags: Flag,
};

const props = withDefaults(defineProps<{ selected?: string[] }>(), { selected: () => [] });

const emit = defineEmits<{ select: [emoji: string]; close: [] }>();

const rootRef = ref<HTMLElement | null>(null);
const searchRef = ref<HTMLElement | null>(null);
const gridRef = ref<HTMLElement | null>(null);

const loadError = ref(false);
const query = ref('');

async function load() {
  loadError.value = false;
  try {
    await loadEmojiData();
  } catch {
    loadError.value = true;
  }
}

const isSearching = computed(() => query.value.trim().length > 0);
const selectedKeys = computed(() => new Set(props.selected.map(emojiKey)));

/**
 * 「よく使う」は開いた時の回数で固定する。`localStorage` は反応しないので、データが同じ間は
 * この computed が再計算されず、開いている間に並びが変わらない。
 */
const frequent = computed(() =>
  emojiDataset.value ? frequentEmojiEntries(emojiDataset.value, readEmojiUseCounts()) : [],
);

const results = computed(() =>
  emojiDataset.value && isSearching.value ? searchEmoji(emojiDataset.value, query.value) : [],
);

const sections = computed(() => {
  const dataset = emojiDataset.value;
  if (!dataset) return [];
  if (isSearching.value) {
    return results.value.length > 0
      ? [
          {
            key: 'search',
            label: `検索結果 ${results.value.length} 件`,
            category: 'frequent' as EmojiCategoryKey,
            entries: results.value,
          },
        ]
      : [];
  }
  return [
    {
      key: 'frequent',
      label: 'よく使う',
      category: 'frequent' as EmojiCategoryKey,
      entries: frequent.value,
    },
    ...dataset.sections,
  ];
});

/** 矢印キーの移動のために、全セクションを通した行の番号を振る */
const layout = computed(() => {
  let rowIndex = 0;
  return sections.value.map((section) => {
    const rows: { index: number; cells: EmojiEntry[] }[] = [];
    for (let start = 0; start < section.entries.length; start += COLUMNS) {
      rows.push({ index: rowIndex++, cells: section.entries.slice(start, start + COLUMNS) });
    }
    return { ...section, rows };
  });
});

const flatRows = computed(() => layout.value.flatMap((section) => section.rows));

const activeRow = ref(0);
const activeCol = ref(0);
const hovered = ref<EmojiEntry | null>(null);

const activeEntry = computed(() => flatRows.value[activeRow.value]?.cells[activeCol.value] ?? null);
const preview = computed(() => hovered.value ?? activeEntry.value);

watch(query, () => {
  activeRow.value = 0;
  activeCol.value = 0;
  hovered.value = null;
  if (gridRef.value) gridRef.value.scrollTop = 0;
});

function choose(entry: EmojiEntry) {
  recordEmojiUse(entry.emoji);
  emit('select', entry.emoji);
}

function cellElement(row: number, col: number) {
  return gridRef.value?.querySelector<HTMLElement>(`[data-row="${row}"][data-col="${col}"]`);
}

function moveTo(row: number, col: number) {
  activeRow.value = row;
  activeCol.value = col;
  hovered.value = null;
  // tabindex の付け替えより先でも focus できるよう、要素は行と列で探す
  void nextTick(() => cellElement(row, col)?.focus());
}

function focusSearch() {
  searchRef.value?.querySelector('input')?.focus();
}

function cellFromEvent(event: Event) {
  const cell = (event.target as Element | null)?.closest?.<HTMLElement>('[role="gridcell"]');
  if (!cell) return null;
  const row = Number(cell.dataset.row);
  const col = Number(cell.dataset.col);
  const entry = flatRows.value[row]?.cells[col];
  return entry ? { row, col, entry } : null;
}

function onGridClick(event: MouseEvent) {
  const cell = cellFromEvent(event);
  if (cell) choose(cell.entry);
}

function onGridPointerOver(event: PointerEvent) {
  hovered.value = cellFromEvent(event)?.entry ?? null;
}

function onGridFocusIn(event: FocusEvent) {
  const cell = cellFromEvent(event);
  if (!cell) return;
  activeRow.value = cell.row;
  activeCol.value = cell.col;
}

function onGridKeyDown(event: KeyboardEvent) {
  const row = activeRow.value;
  const col = activeCol.value;
  const rows = flatRows.value;
  const current = rows[row];
  if (!current) return;
  const moves: Record<string, () => void> = {
    ArrowRight: () => {
      const next = rows[row + 1];
      if (col + 1 < current.cells.length) moveTo(row, col + 1);
      else if (next) moveTo(row + 1, 0);
    },
    ArrowLeft: () => {
      const previous = rows[row - 1];
      if (col > 0) moveTo(row, col - 1);
      else if (previous) moveTo(row - 1, previous.cells.length - 1);
    },
    ArrowDown: () => {
      const next = rows[row + 1];
      if (next) moveTo(row + 1, Math.min(col, next.cells.length - 1));
    },
    ArrowUp: () => {
      const previous = rows[row - 1];
      if (previous) moveTo(row - 1, Math.min(col, previous.cells.length - 1));
      else focusSearch();
    },
    Home: () => moveTo(row, 0),
    End: () => moveTo(row, current.cells.length - 1),
    Enter: () => {
      if (activeEntry.value) choose(activeEntry.value);
    },
  };
  const move = moves[event.key];
  if (!move) return;
  event.preventDefault();
  move();
}

function onSearchKeyDown(event: KeyboardEvent) {
  // 日本語の変換を確定する Enter で選ばない
  if (event.isComposing || event.keyCode === 229) return;
  if (event.key === 'Enter') {
    event.preventDefault();
    const first = results.value[0];
    if (first) choose(first);
  } else if (event.key === 'ArrowDown') {
    event.preventDefault();
    if (flatRows.value.length > 0) moveTo(activeRow.value, activeCol.value);
  }
}

function onRootKeyDown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || event.isComposing || event.keyCode === 229) return;
  // タスク詳細の dialog まで閉じないよう止める
  event.preventDefault();
  event.stopPropagation();
  emit('close');
}

// ─── カテゴリの列 ───

const activeCategory = ref<EmojiCategoryKey>('frequent');

function sectionElements() {
  return Array.from(gridRef.value?.querySelectorAll<HTMLElement>('[data-section]') ?? []);
}

function onGridScroll() {
  const grid = gridRef.value;
  if (!grid || isSearching.value) return;
  const top = grid.scrollTop + 1;
  const current = sectionElements().findLast((element) => element.offsetTop <= top);
  const section = sections.value.find((item) => item.key === current?.dataset.section);
  if (section) activeCategory.value = section.category;
}

function jumpToCategory(key: EmojiCategoryKey) {
  const section = sections.value.find((item) => item.category === key);
  const element = sectionElements().find((item) => item.dataset.section === section?.key);
  if (!gridRef.value || !element) return;
  gridRef.value.scrollTop = element.offsetTop;
  activeCategory.value = key;
}

function sectionHeight(rowCount: number) {
  return `${HEADING_HEIGHT_PX + rowCount * ROW_HEIGHT_PX}px`;
}

onMounted(() => {
  void load();
  // タッチ端末で開くたびにソフトキーボードが出ないよう、検索欄へは入らない
  const touch = window.matchMedia?.('(hover: none)').matches;
  if (touch) rootRef.value?.focus({ preventScroll: true });
  else focusSearch();
});
</script>

<template>
  <div
    ref="rootRef"
    role="dialog"
    aria-label="絵文字を選ぶ"
    tabindex="-1"
    class="flex h-[440px] max-h-[calc(100dvh-16px)] w-[360px] max-w-[calc(100vw-16px)] flex-col overflow-hidden rounded-[16px] border border-(--koyori-color-border) bg-(--koyori-color-surface) text-(--koyori-color-text) shadow-(--koyori-shadow-panel) outline-none"
    @keydown="onRootKeyDown"
  >
    <div
      ref="searchRef"
      class="shrink-0 border-b border-(--koyori-color-border) p-2"
      @keydown="onSearchKeyDown"
    >
      <Input
        :value="query"
        type="search"
        aria-label="絵文字を検索"
        placeholder="絵文字を検索"
        autocomplete="off"
        @value-change="query = $event"
      >
        <template #prefix>
          <Search :size="16" aria-hidden="true" />
        </template>
      </Input>
    </div>

    <div class="flex min-h-0 flex-1">
      <div
        v-if="!isSearching"
        role="group"
        aria-label="カテゴリ"
        class="flex w-11 shrink-0 flex-col items-center gap-0.5 overflow-y-auto border-r border-(--koyori-color-border) py-1"
      >
        <button
          v-for="category in EMOJI_CATEGORIES"
          :key="category.key"
          type="button"
          class="rail-button"
          :aria-label="category.label"
          :title="category.label"
          :aria-current="activeCategory === category.key ? 'true' : undefined"
          @click="jumpToCategory(category.key)"
        >
          <component :is="CATEGORY_ICONS[category.key]" :size="18" aria-hidden="true" />
        </button>
      </div>

      <div v-if="loadError" role="alert" class="m-auto px-4 text-center text-sm">
        <p class="mb-2 text-destructive">絵文字を読み込めませんでした</p>
        <Button variant="secondary" label="もう一度読み込む" @click="load" />
      </div>
      <div v-else-if="!emojiDataset" class="min-w-0 flex-1 p-2" aria-busy="true">
        <Skeleton width="30%" height="14px" />
        <div class="mt-2 grid grid-cols-8 gap-1">
          <Skeleton v-for="index in 40" :key="index" height="28px" radius="8px" />
        </div>
      </div>
      <div
        v-else-if="isSearching && results.length === 0"
        class="m-auto px-4 text-center text-sm text-(--koyori-color-text-secondary)"
      >
        <p class="font-medium text-(--koyori-color-text)">絵文字が見つかりません</p>
        <p class="mt-1 text-xs">別の言葉で検索してください</p>
      </div>
      <div
        v-else
        ref="gridRef"
        role="grid"
        aria-label="絵文字"
        class="emoji-grid relative min-w-0 flex-1 overflow-y-auto px-2"
        @click="onGridClick"
        @pointerover="onGridPointerOver"
        @pointerleave="hovered = null"
        @focusin="onGridFocusIn"
        @keydown="onGridKeyDown"
        @scroll.passive="onGridScroll"
      >
        <div
          v-for="section in layout"
          :key="section.key"
          role="rowgroup"
          class="emoji-section"
          :data-section="section.key"
          :style="{ containIntrinsicHeight: `auto ${sectionHeight(section.rows.length)}` }"
        >
          <div role="row" class="emoji-heading">
            <span role="columnheader" :aria-colspan="COLUMNS">{{ section.label }}</span>
          </div>
          <div
            v-for="row in section.rows"
            :key="row.index"
            v-memo="[row.cells, activeRow === row.index ? activeCol : -1, selectedKeys]"
            role="row"
            class="emoji-row"
          >
            <button
              v-for="(entry, col) in row.cells"
              :key="entry.hexcode"
              type="button"
              role="gridcell"
              class="cell"
              :data-row="row.index"
              :data-col="col"
              :data-selected="selectedKeys.has(emojiKey(entry.emoji)) || undefined"
              :aria-selected="selectedKeys.has(emojiKey(entry.emoji))"
              :aria-label="entry.label"
              :tabindex="activeRow === row.index && activeCol === col ? 0 : -1"
            >
              {{ entry.emoji }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="sr-only" aria-live="polite">
      {{ isSearching && emojiDataset ? `検索結果 ${results.length} 件` : '' }}
    </div>

    <div
      class="flex h-[47px] shrink-0 items-center gap-2 border-t border-(--koyori-color-border) px-3"
      data-testid="emoji-preview"
    >
      <template v-if="preview">
        <span class="text-2xl leading-none" aria-hidden="true">{{ preview.emoji }}</span>
        <span class="min-w-0 truncate text-sm">{{ preview.label }}</span>
        <code
          v-if="preview.shortcode"
          class="ml-auto min-w-0 truncate font-mono text-xs text-(--koyori-color-text-secondary)"
          >:{{ preview.shortcode }}:</code
        >
      </template>
    </div>
  </div>
</template>

<style scoped>
.rail-button {
  display: inline-flex;
  width: 32px;
  height: 32px;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  color: var(--koyori-color-text-secondary);
}

.rail-button[aria-current='true'] {
  background: var(--koyori-color-selection);
  color: var(--koyori-color-text);
}

.emoji-grid {
  scrollbar-width: thin;
}

/* 1,800 個近い字の描画を、見えている分だけに抑える */
.emoji-section {
  content-visibility: auto;
}

.emoji-heading {
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  height: 28px;
  align-items: center;
  background: var(--koyori-color-surface);
  color: var(--koyori-color-text-secondary);
  font-size: 12px;
  font-weight: 500;
}

.emoji-row {
  display: grid;
  grid-template-columns: repeat(8, minmax(0, 36px));
}

.cell {
  display: inline-flex;
  height: 36px;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  font-size: 22px;
  line-height: 1;
  /* フォーカスで動いた升が見出しの下に隠れないように */
  scroll-margin-top: 28px;
}

.cell[data-selected] {
  background: var(--koyori-color-selection);
}

@media (hover: hover) {
  .rail-button:not([aria-current='true']):hover {
    background: var(--koyori-color-surface-hover);
  }

  .cell:hover {
    background: var(--koyori-color-surface-hover);
  }

  .cell[data-selected]:hover {
    background: var(--koyori-color-accent-hover);
  }
}

.rail-button:focus-visible,
.cell:focus-visible {
  outline: var(--koyori-focus-width) solid var(--koyori-focus-color);
  outline-offset: calc(-1 * var(--koyori-focus-width));
}
</style>
