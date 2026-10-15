<script setup lang="ts">
import { ChevronLeft, ChevronRight, Flag, TriangleAlert } from '@lucide/vue';
import { computed } from 'vue';
import { navigate } from 'vike/client/router';

import type { components } from '@/generated/api';
import { DAILY_CAPACITY_HOURS, dailyLoadByMember, isOverCapacity } from '@/lib/calendar/load';
import {
  clipSpan,
  dateInZone,
  isWeekend,
  pack,
  rangeDates,
  taskSpan,
  type ClippedSpan,
} from '@/lib/calendar/timeline';
import { taskDetailHref } from '@/lib/task-display';

type ScheduleTask = components['schemas']['ScheduleTask'];
type Member = components['schemas']['UserSummary'];
type Sprint = components['schemas']['SprintResponse'];
type Milestone = components['schemas']['MilestoneResponse'];

const props = withDefaults(
  defineProps<{
    from: string;
    days?: number;
    today: string;
    timeZone: string;
    tenantSlug: string;
    tasks: ScheduleTask[];
    members: Member[];
    sprints?: Sprint[];
    milestones?: Milestone[];
    /** 担当者のいないタスクの行を出すか（メンバーで絞り込んでいるときは出さない） */
    showUnassigned?: boolean;
  }>(),
  { days: 14, sprints: () => [], milestones: () => [], showUnassigned: true },
);

const WEEKDAYS = ['日', '月', '火', '水', '木', '金', '土'];

const dates = computed(() => rangeDates(props.from, props.days));
const gridStyle = computed(() => ({
  gridTemplateColumns: `10rem repeat(${props.days}, minmax(4rem, 1fr))`,
}));

function dayLabel(date: string) {
  const [, month, day] = date.split('-');
  const weekday = WEEKDAYS[new Date(`${date}T00:00:00Z`).getUTCDay()];
  return { short: `${Number(month)}/${Number(day)}`, weekday };
}

function columnClass(date: string) {
  if (date === props.today) return 'bg-primary/10';
  return isWeekend(date) ? 'bg-muted/60' : '';
}

type Placed<T> = ClippedSpan & { start: number; end: number; item: T };

function place<T>(item: T, start: string, end: string): Placed<T> | null {
  const clipped = clipSpan({ start, end }, props.from, props.days);
  return clipped ? { ...clipped, start: clipped.startCol, end: clipped.endCol, item } : null;
}

// ---- 上段の帯（スプリント・マイルストーン） ----
type BandItem =
  | { kind: 'sprint'; id: string; name: string }
  | { kind: 'milestone'; id: string; name: string };

const bandLanes = computed(() => {
  const placed: Placed<BandItem>[] = [];
  for (const s of props.sprints) {
    const p = place<BandItem>(
      { kind: 'sprint', id: s.id, name: s.name },
      s.start_date.slice(0, 10),
      s.end_date.slice(0, 10),
    );
    if (p) placed.push(p);
  }
  for (const m of props.milestones) {
    const due = m.due_date.slice(0, 10);
    const p = place<BandItem>({ kind: 'milestone', id: m.id, name: m.name }, due, due);
    if (p) placed.push(p);
  }
  return pack(placed);
});

// ---- メンバーの行 ----
type Entry = {
  task: ScheduleTask;
  primary: boolean;
  /** 開始予定日の無いタスクは締切の点で描く */
  point: boolean;
};

const UNASSIGNED_ID = '__unassigned__';

const rows = computed(() => {
  const rowsById = new Map<string, Placed<Entry>[]>();
  const add = (rowId: string, entry: Placed<Entry>) => {
    const list = rowsById.get(rowId) ?? [];
    list.push(entry);
    rowsById.set(rowId, list);
  };
  for (const task of props.tasks) {
    const span = taskSpan(task, props.timeZone);
    if (!span) continue;
    const point = !task.planned_start_on;
    if (!task.assignees.length) {
      const p = place<Entry>({ task, primary: true, point }, span.start, span.end);
      if (p) add(UNASSIGNED_ID, p);
      continue;
    }
    for (const assignee of task.assignees) {
      const p = place<Entry>(
        { task, primary: assignee.role === 'primary', point },
        span.start,
        span.end,
      );
      if (p) add(assignee.user_id, p);
    }
  }
  const loadByMember = dailyLoadByMember(props.tasks, props.timeZone);
  const loadsOf = (memberId: string) =>
    dates.value.flatMap((date, col) => {
      const hours = loadByMember.get(memberId)?.get(date) ?? 0;
      return hours > 0 ? [{ date, col, hours, over: isOverCapacity(hours) }] : [];
    });
  const result = props.members.map((member) => ({
    id: member.id,
    name: member.username,
    lanes: pack(rowsById.get(member.id) ?? []),
    loads: loadsOf(member.id),
  }));
  const unassigned = rowsById.get(UNASSIGNED_ID);
  if (props.showUnassigned && unassigned) {
    result.push({ id: UNASSIGNED_ID, name: '担当者なし', lanes: pack(unassigned), loads: [] });
  }
  return result;
});

function loadLabel(hours: number) {
  return `${Math.round(hours * 10) / 10}h`;
}

function loadAria(date: string, hours: number) {
  const base = `${date} の見積 ${loadLabel(hours)}`;
  return isOverCapacity(hours) ? `${base}（上限 ${DAILY_CAPACITY_HOURS}h を超過）` : base;
}

function isOverdue(task: ScheduleTask) {
  return (
    !task.status.is_done_state &&
    !!task.soft_deadline &&
    dateInZone(task.soft_deadline, props.timeZone) < props.today
  );
}

function entryClass(entry: Entry) {
  const tone = entry.task.status.is_done_state
    ? 'done'
    : isOverdue(entry.task)
      ? 'overdue'
      : 'normal';
  if (entry.point) {
    return {
      done: 'text-muted-foreground',
      overdue: 'text-destructive',
      normal: 'text-foreground',
    }[tone];
  }
  if (entry.primary) {
    return {
      done: 'border-border bg-muted text-muted-foreground',
      overdue: 'border-destructive/50 bg-destructive/15 text-destructive',
      normal: 'border-primary/40 bg-primary/15 text-foreground',
    }[tone];
  }
  // サブ担当は塗らずに薄い枠線だけ
  return {
    done: 'border-dashed border-border text-muted-foreground',
    overdue: 'border-dashed border-destructive/40 text-destructive',
    normal: 'border-dashed border-primary/30 text-foreground',
  }[tone];
}

function diamondClass(entry: Entry) {
  if (entry.task.status.is_done_state) return 'bg-muted-foreground/60';
  if (isOverdue(entry.task)) return 'bg-destructive';
  return entry.primary ? 'bg-primary' : 'border border-primary bg-background';
}

function entryAria(entry: Entry) {
  const role = entry.primary ? '主担当' : 'サブ担当';
  const state = entry.task.status.is_done_state
    ? '・完了'
    : isOverdue(entry.task)
      ? '・期限超過'
      : '';
  return `${entry.task.seq_key} ${entry.task.title}（${role}${state}）`;
}

function hrefOf(task: ScheduleTask) {
  return taskDetailHref(props.tenantSlug, task.project.key, task.seq_id);
}

function onTaskClick(task: ScheduleTask, event: MouseEvent) {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey)
    return;
  event.preventDefault();
  void navigate(hrefOf(task));
}

function cellStyle(col: number, row: string) {
  return { gridColumn: `${col + 2}`, gridRow: row };
}

function spanStyle(placed: { startCol: number; endCol: number }, row: number) {
  return { gridColumn: `${placed.startCol + 2} / ${placed.endCol + 3}`, gridRow: `${row}` };
}
</script>

<template>
  <div class="overflow-x-auto rounded-md border" data-calendar-timeline>
    <div :style="{ minWidth: `${10 + days * 4}rem` }">
      <!-- 日付の見出し -->
      <div class="grid border-b text-xs" :style="gridStyle">
        <div class="sticky left-0 z-10 border-r bg-background px-2 py-1.5 text-muted-foreground">
          メンバー
        </div>
        <div
          v-for="date in dates"
          :key="date"
          class="border-r px-1 py-1.5 text-center last:border-r-0"
          :class="[columnClass(date), date === today ? 'font-semibold text-primary' : '']"
          :aria-current="date === today ? 'date' : undefined"
          :data-date="date"
        >
          <div>{{ dayLabel(date).short }}</div>
          <div :class="date === today ? '' : 'text-muted-foreground'">
            {{ dayLabel(date).weekday }}
          </div>
        </div>
      </div>

      <!-- スプリント・マイルストーン -->
      <div
        v-if="bandLanes.length"
        class="grid border-b"
        :style="{ ...gridStyle, gridTemplateRows: `repeat(${bandLanes.length}, 1.5rem)` }"
        data-calendar-band
      >
        <div
          class="sticky left-0 z-10 flex items-center border-r bg-background px-2 text-xs text-muted-foreground"
          style="grid-column: 1; grid-row: 1 / -1"
        >
          スプリント
        </div>
        <div
          v-for="(date, col) in dates"
          :key="date"
          class="border-r last:border-r-0"
          :class="columnClass(date)"
          :style="cellStyle(col, '1 / -1')"
        />
        <template v-for="(lane, laneIndex) in bandLanes" :key="laneIndex">
          <div
            v-for="placed in lane"
            :key="placed.item.id"
            class="relative z-[1] mx-0.5 my-0.5 flex min-w-0 items-center gap-1 overflow-hidden text-xs"
            :class="
              placed.item.kind === 'sprint'
                ? 'rounded border bg-secondary px-1.5 text-secondary-foreground'
                : 'px-1 text-foreground'
            "
            :style="spanStyle(placed, laneIndex + 1)"
            :data-kind="placed.item.kind"
          >
            <ChevronLeft v-if="placed.clippedStart" class="size-3 shrink-0" aria-hidden="true" />
            <Flag
              v-if="placed.item.kind === 'milestone'"
              class="size-3 shrink-0 text-primary"
              aria-label="マイルストーン"
            />
            <span class="truncate">{{ placed.item.name }}</span>
            <ChevronRight
              v-if="placed.clippedEnd"
              class="ml-auto size-3 shrink-0"
              aria-hidden="true"
            />
          </div>
        </template>
      </div>

      <p
        v-if="!tasks.length"
        class="px-4 py-12 text-center text-sm text-muted-foreground"
        data-calendar-empty
      >
        この期間のタスクはありません
      </p>

      <!-- メンバーごとの行 -->
      <template v-else>
        <div
          v-for="row in rows"
          :key="row.id"
          class="grid border-b last:border-b-0"
          :style="{
            ...gridStyle,
            gridTemplateRows: `1rem repeat(${Math.max(row.lanes.length, 1)}, 1.75rem)`,
          }"
          :data-member-row="row.id"
        >
          <div
            class="sticky left-0 z-10 flex items-center border-r bg-background px-2 text-sm"
            style="grid-column: 1; grid-row: 1 / -1"
          >
            <span class="truncate">{{ row.name }}</span>
          </div>
          <div
            v-for="(date, col) in dates"
            :key="date"
            class="border-r last:border-r-0"
            :class="columnClass(date)"
            :style="cellStyle(col, '1 / -1')"
          />
          <!-- 見積/日（主担当・開始予定日と Soft 締切あり・未完了のタスクから） -->
          <div
            v-for="load in row.loads"
            :key="`load-${load.date}`"
            class="relative z-[1] flex items-center justify-end gap-0.5 px-1 text-[10px] leading-none"
            :class="load.over ? 'font-medium text-destructive' : 'text-muted-foreground'"
            :style="cellStyle(load.col, '1')"
            :aria-label="loadAria(load.date, load.hours)"
            :data-load-date="load.date"
            :data-over="load.over || undefined"
            role="note"
          >
            <TriangleAlert v-if="load.over" class="size-2.5" aria-hidden="true" />
            <span aria-hidden="true"
              >{{ loadLabel(load.hours) }}{{ load.over ? ' 超過' : '' }}</span
            >
          </div>
          <template v-for="(lane, laneIndex) in row.lanes" :key="laneIndex">
            <a
              v-for="placed in lane"
              :key="placed.item.task.id"
              :href="hrefOf(placed.item.task)"
              class="relative z-[1] mx-0.5 my-0.5 flex min-w-0 items-center gap-1 overflow-hidden rounded text-xs hover:opacity-80 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
              :class="[placed.item.point ? 'px-1' : 'border px-1.5', entryClass(placed.item)]"
              :style="spanStyle(placed, laneIndex + 2)"
              :aria-label="entryAria(placed.item)"
              :title="entryAria(placed.item)"
              :data-role="placed.item.primary ? 'primary' : 'sub'"
              :data-shape="placed.item.point ? 'point' : 'bar'"
              :data-task-id="placed.item.task.id"
              @click="onTaskClick(placed.item.task, $event)"
            >
              <ChevronLeft v-if="placed.clippedStart" class="size-3 shrink-0" aria-hidden="true" />
              <span
                v-if="placed.item.point"
                class="size-2 shrink-0 rotate-45"
                :class="diamondClass(placed.item)"
                aria-hidden="true"
              />
              <span class="truncate">{{ placed.item.task.title }}</span>
              <ChevronRight
                v-if="placed.clippedEnd"
                class="ml-auto size-3 shrink-0"
                aria-hidden="true"
              />
            </a>
          </template>
        </div>
      </template>
    </div>
  </div>
</template>
