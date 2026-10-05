<script lang="ts">
  import { MediaQuery } from 'svelte/reactivity';
  import { agendaEntries } from '../lib/agenda';
  import { isFinished } from '../lib/colors';
  import { dueKey, monthCells, todayKey } from '../lib/format';
  import { horizontalSwipe } from '../lib/horizontal-swipe';
  import { todoKey } from '../lib/todos.svelte';
  import type { CalendarItem, Todo } from '../lib/types';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import MonthWheel from './MonthWheel.svelte';

  let {
    year = $bindable(),
    month = $bindable(),
    selected = $bindable(),
    items,
    colors,
    names,
    todos = [],
    week = false,
    collapse = 0,
    onpick,
    onexpand,
  }: {
    year: number;
    month: number;
    selected: string;
    items: CalendarItem[];
    colors: Map<number, string>;
    names: Map<number, string>;
    todos?: Todo[];
    week?: boolean;
    collapse?: number;
    onpick?: (key: string, el: HTMLElement) => void;
    onexpand?: () => void;
  } = $props();

  const maxShow = 3;
  const narrow = new MediaQuery('max-width: 767px');
  const small = new MediaQuery('max-width: 374px');
  let monthPickerOpen = $state(false);
  let pickerYear = $state(0);
  let pickerMonth = $state(1);

  function openMonthPicker() {
    pickerYear = year;
    pickerMonth = month;
    monthPickerOpen = true;
  }

  function chooseMonth() {
    const lastDay = new Date(Date.UTC(pickerYear, pickerMonth, 0)).getUTCDate();
    const day = Math.min(Number(selected.slice(8)), lastDay);
    year = pickerYear;
    month = pickerMonth;
    selected = `${year}-${String(month).padStart(2, '0')}-${String(day).padStart(2, '0')}`;
    monthPickerOpen = false;
  }

  const todosByDay = $derived.by(() => {
    const map = new Map<string, Todo[]>();
    for (const t of todos) {
      const key = todoKey(t);
      if (!key) continue;
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(t);
    }
    return map;
  });

  function short(item: CalendarItem) {
    const name = names.get(item.courseId) ?? '';
    const head = name.split(/[\s(]/)[0];
    for (const prefix of [name, head]) {
      if (prefix && item.title.startsWith(prefix)) {
        const rest = item.title.slice(prefix.length).replace(/^[\s\-:·_)]+/, '');
        if (rest) return rest;
      }
    }
    return item.title;
  }

  const DAY = 86_400_000;
  const keyOf = (d: Date) =>
    `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
  const toDate = (key: string) => {
    const [y, m, d] = key.split('-').map(Number);
    return new Date(Date.UTC(y, m - 1, d));
  };

  function pageCells(offset: number) {
    if (week) {
      const sel = toDate(selected);
      const start = sel.getTime() - sel.getUTCDay() * DAY + offset * 7 * DAY;
      return Array.from({ length: 7 }, (_, i) => {
        const d = new Date(start + i * DAY);
        return { key: keyOf(d), inMonth: true };
      });
    }
    const date = new Date(Date.UTC(year, month - 1 + offset, 1));
    const all = monthCells(date.getUTCFullYear(), date.getUTCMonth() + 1);
    const rows = [];
    for (let i = 0; i < all.length; i += 7) {
      const row = all.slice(i, i + 7);
      if (row.some((c) => c.inMonth)) rows.push(...row);
    }
    return rows;
  }
  const cells = $derived(pageCells(0));
  const pages = $derived((narrow.current ? [-1, 0, 1] : [0]).map((offset) => ({ offset, cells: offset === 0 ? cells : pageCells(offset) })));
  let pagesElement: HTMLDivElement | undefined = $state();
  let selectedRowTop = $state(0);
  $effect(() => {
    cells;
    selected;
    if (!pagesElement) return;
    const element = pagesElement;
    const measure = () => { selectedRowTop = element.querySelector<HTMLElement>('.current .sel')?.offsetTop ?? 0; };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  });
  const today = $derived(todayKey());

  const byDay = $derived.by(() => {
    const map = new Map<string, CalendarItem[]>();
    for (const item of items) {
      if (item.due === null) continue;
      const key = dueKey(item.due);
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(item);
    }
    return map;
  });

  function follow(key: string) {
    const [y, m] = key.split('-').map(Number);
    year = y;
    month = m;
  }

  function shift(delta: number) {
    if (week) {
      const d = new Date(toDate(selected).getTime() + delta * 7 * DAY);
      selected = keyOf(d);
      follow(selected);
      return;
    }
    const d = new Date(Date.UTC(year, month - 1 + delta, 1));
    year = d.getUTCFullYear();
    month = d.getUTCMonth() + 1;
  }

  function goToday() {
    follow(today);
    selected = today;
  }

  function pick(e: MouseEvent, key: string, inMonth: boolean) {
    selected = key;
    if (!inMonth || week) follow(key);
    onpick?.(key, e.currentTarget as HTMLElement);
  }

  function label(key: string, list: CalendarItem[]) {
    const [, m, d] = key.split('-').map(Number);
    const left = list.filter((i) => !isFinished(i)).length;
    return `${m}월 ${d}일, 일정 ${list.length}개${left ? `, 남은 일 ${left}개` : ''}`;
  }

  const weekTitle = $derived.by(() => {
    if (!week) return '';
    const [a, b] = [cells[0].key, cells[6].key].map((k) => k.split('-').map(Number));
    return `${a[1]}월 ${a[2]}일 – ${b[1]}월 ${b[2]}일`;
  });
  const compactWeekTitle = $derived.by(() => {
    if (!week) return '';
    const [a, b] = [cells[0].key, cells[6].key].map((key) => key.split('-').map(Number));
    return `${a[1]}/${a[2]}–${b[1]}/${b[2]}`;
  });
</script>

<div class="cal card" class:week use:horizontalSwipe={{ enabled: () => narrow.current, shift }}>
  <div class="head">
    <button class="icon-btn" onclick={() => shift(-1)} aria-label={week ? '이전 주' : '이전 달'}><Icon name="left" /></button>
    <h2 aria-live="polite" aria-label={week ? weekTitle : undefined}>
      {#if week}
        {small.current ? compactWeekTitle : weekTitle}
      {:else}
        <button class="month-title" onclick={openMonthPicker} aria-label="{year}년 {month}월, 년·월 선택" aria-haspopup="dialog">{year}년 {month}월</button>
      {/if}
    </h2>
    <button class="icon-btn" onclick={() => shift(1)} aria-label={week ? '다음 주' : '다음 달'}><Icon name="right" /></button>
    {#if week}
      <button class="today-btn" onclick={() => onexpand?.()} aria-label="캘린더 펼치기">
        캘린더
      </button>
    {:else}
      <button class="today-btn" onclick={goToday}>오늘</button>
    {/if}
  </div>
  <div class="weekdays" aria-hidden="true">
    {#each ['일', '월', '화', '수', '목', '금', '토'] as w, i (w)}
      <div class="wd" class:sun={i === 0} class:sat={i === 6} aria-hidden="true">{w}</div>
    {/each}
  </div>
  <div class="calendar-window">
    <div class="calendar-vertical" style:transform="translate3d(0, {-selectedRowTop * collapse}px, 0)">
    <div class="calendar-pages" bind:this={pagesElement}>
      {#each pages as page (page.offset)}
  <div class="grid calendar-page" class:previous={page.offset === -1} class:next={page.offset === 1} class:current={page.offset === 0} inert={page.offset !== 0} aria-hidden={page.offset !== 0} role="group" aria-label={week ? weekTitle : `${year}년 ${month}월`}>
    {#each page.cells as cell, i (cell.key)}
      {@const list = byDay.get(cell.key) ?? []}
      {@const tlist = todosByDay.get(cell.key) ?? []}
      {@const entries = agendaEntries(list, tlist)}
      <button
        class="day"
        class:out={!cell.inMonth}
        class:today={cell.key === today}
        class:sel={cell.key === selected}
        class:sun={i % 7 === 0}
        class:sat={i % 7 === 6}
        onclick={(e) => pick(e, cell.key, cell.inMonth)}
        aria-label="{label(cell.key, list)}{tlist.length ? `, 할 일 ${tlist.length}개` : ''}"
        aria-pressed={cell.key === selected}
      >
        <span class="num">{Number(cell.key.slice(8))}</span>
        <span class="evs">
          {#each entries.slice(0, maxShow) as entry (entry.key)}
            <span
              class="ev"
              class:finished={entry.done}
              class:todo={entry.kind === 'todo'}
              class:vod={entry.kind === 'item' && entry.value.kind === 'vod'}
              style:--c={entry.kind === 'todo' ? (entry.value.courseId === null ? 'var(--todo-neutral)' : (colors.get(entry.value.courseId) ?? 'var(--todo-neutral)')) : (colors.get(entry.value.courseId) ?? 'var(--text-3)')}
            >
              <span class="t">{entry.kind === 'todo' ? entry.value.title : short(entry.value)}</span>
            </span>
          {/each}
          {#if list.length + tlist.length > maxShow}<span class="more">+{list.length + tlist.length - maxShow}</span>{/if}
        </span>
      </button>
    {/each}
  </div>
      {/each}
    </div>
    </div>
  </div>
</div>

<Sheet bind:open={monthPickerOpen} title="년·월 선택">
  <MonthWheel bind:year={pickerYear} bind:month={pickerMonth} />
  {#snippet footer()}<button class="btn btn-primary w1" onclick={chooseMonth}>확인</button>{/snippet}
</Sheet>

<style>
  .month-title { min-height: 40px; font-weight: inherit; letter-spacing: inherit; }

  .cal {
    padding: 14px 10px 12px;
    overflow-anchor: none;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0 2px 8px;
  }

  h2 {
    font-size: 18px;
    font-weight: 750;
    letter-spacing: -0.02em;
    min-width: 120px;
    text-align: center;
  }

  .today-btn {
    margin-left: auto;
    height: 40px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .calendar-window { overflow: hidden; }
  .calendar-pages { position: relative; }
  .calendar-page.previous, .calendar-page.next { position: absolute; top: 0; width: 100%; }
  .calendar-page.previous { right: 100%; }
  .calendar-page.next { left: 100%; }

  .grid, .weekdays {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 2px;
  }

  .wd {
    text-align: center;
    font-size: 12px;
    font-weight: 650;
    color: var(--text-3);
    padding: 4px 0 6px;
  }

  .sun {
    --daycolor: var(--danger);
  }

  .sat {
    --daycolor: var(--info);
  }

  .wd.sun,
  .wd.sat {
    color: var(--daycolor);
  }

  .day {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    min-height: 54px;
    padding: 3px 2px 4px;
    border-radius: 12px;
    transition: background 0.15s;
  }

  .day:hover {
    background: var(--surface-2);
  }

  .num {
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 999px;
    font-size: 14px;
    font-weight: 600;
    color: var(--daycolor, var(--text));
    font-variant-numeric: tabular-nums;
  }

  .out .num {
    color: var(--text-3);
  }

  .today .num {
    background: var(--primary-weak);
    color: var(--primary-text);
    font-weight: 800;
  }

  .sel .num {
    background: var(--primary);
    color: var(--on-primary);
    font-weight: 800;
  }

  .evs {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 3px;
    width: 100%;
  }

  .ev {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--c);
  }

  .ev.vod {
    background: transparent;
    box-shadow: inset 0 0 0 1.6px var(--c);
  }

  .ev.todo {
    border-radius: 2px;
  }

  .ev.finished {
    color: var(--text-3);
  }

  .ev .t {
    display: none;
  }

  .more {
    font-size: 10px;
    line-height: 6px;
    color: var(--text-3);
    font-weight: 700;
  }

  .cal.week {
    padding: 8px 8px 6px;
    border-radius: 0 0 var(--radius) var(--radius);
    box-shadow: var(--shadow);
  }

  .week .head {
    margin-bottom: 2px;
  }

  .week h2 {
    font-size: 15px;
  }

  .week .wd {
    padding: 0 0 2px;
    font-size: 11px;
  }

  .week .day {
    min-height: 46px;
    padding: 3px 2px;
    gap: 3px;
  }

  @media (max-width: 767px) {
    .cal {
      touch-action: pan-y pinch-zoom;
    }

    h2 { min-width: 0; flex: 1; white-space: nowrap; }
    .today-btn { flex-shrink: 0; white-space: nowrap; padding-inline: 8px; }
  }

  @media (min-width: 768px) {
    .cal:not(.week) .day {
      align-items: stretch;
      min-height: 100px;
      padding: 3px 6px 6px;
    }

    .cal:not(.week) .num {
      width: 26px;
      height: 26px;
      font-size: 13px;
    }

    .cal:not(.week) .evs {
      flex-direction: column;
      flex-wrap: nowrap;
      gap: 3px;
    }

    .cal:not(.week) .ev,
    .cal:not(.week) .ev.vod,
    .cal:not(.week) .ev.todo {
      width: 100%;
      height: auto;
      border-radius: 6px;
      padding: 2px 6px;
      background: color-mix(in srgb, var(--c) 14%, transparent);
      box-shadow: inset 3px 0 0 var(--c);
      text-align: left;
    }

    .cal:not(.week) .ev .t {
      display: block;
      font-size: 11.5px;
      font-weight: 600;
      color: var(--text);
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .cal:not(.week) .ev.finished .t {
      text-decoration: line-through;
    }

    .cal:not(.week) .ev.todo {
      background: transparent;
      box-shadow:
        inset 0 0 0 1px color-mix(in srgb, var(--c) 45%, transparent),
        inset 3px 0 0 var(--c);
    }

    .cal:not(.week) .more {
      line-height: 1.2;
      font-size: 11px;
      text-align: left;
      padding-left: 6px;
    }
  }

  @media (max-width: 359px) {
    h2 { font-size: 16px; }
    .week h2 { font-size: 15px; }
  }
</style>
