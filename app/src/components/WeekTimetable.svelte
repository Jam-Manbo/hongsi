<script lang="ts">
  import { periodLabel } from '../lib/classwatch.svelte';
  import type { ClassSlot } from '../lib/types';
  import Popover from './Popover.svelte';

  let {
    slots,
    colors,
    now = Date.now(),
    compact = false,
  }: {
    slots: ClassSlot[];
    colors: Map<string, string>;
    now?: number;
    compact?: boolean;
  } = $props();

  const DAYS = ['월', '화', '수', '목', '금', '토', '일'];
  const KST = 9 * 3600_000;

  const toMin = (s: ClassSlot) => {
    const [h, m] = s.start.split(':').map(Number);
    return h * 60 + m;
  };

  const today = $derived((new Date(now + KST).getUTCDay() + 6) % 7);
  const nowMin = $derived(Math.floor(((now + KST) % 86_400_000) / 60_000));

  const days = $derived(Array.from({ length: Math.max(4, ...slots.map((s) => s.weekday)) + 1 }, (_, i) => i));

  const range = $derived.by(() => {
    const starts = slots.map(toMin);
    const ends = slots.map((s) => toMin(s) + s.periods.length * 60);
    return {
      from: Math.min(9, ...starts.map((m) => Math.floor(m / 60))),
      to: Math.max(18, ...ends.map((m) => Math.ceil(m / 60))),
    };
  });
  const hours = $derived(Array.from({ length: range.to - range.from }, (_, i) => range.from + i));

  const byDay = $derived.by(() => {
    const map = new Map<number, ClassSlot[]>();
    for (const s of slots) {
      if (!map.has(s.weekday)) map.set(s.weekday, []);
      map.get(s.weekday)!.push(s);
    }
    return map;
  });

  const color = (s: ClassSlot) => colors.get(s.code ?? '') ?? 'var(--text-3)';
  const top = (s: ClassSlot) => (toMin(s) - range.from * 60) / 60;
  const showNow = $derived(nowMin >= range.from * 60 && nowMin < range.to * 60);

  let picked = $state<ClassSlot | null>(null);
  let anchor = $state<HTMLElement | null>(null);
  let popOpen = $state(false);

  function pick(e: MouseEvent, s: ClassSlot) {
    const el = e.currentTarget as HTMLElement;
    if (popOpen && anchor === el) {
      popOpen = false;
      return;
    }
    picked = s;
    anchor = el;
    popOpen = true;
  }
</script>

{#if slots.length}
  <div class="tt" class:compact style:--cols={days.length} role="group" aria-label="주간 시간표">
    <div class="head" aria-hidden="true">
      <span class="corner"></span>
      {#each days as d (d)}
        <span class="dh" class:today={d === today}>{DAYS[d]}</span>
      {/each}
    </div>
    <div class="body" style:--rows={hours.length}>
      <div class="hours" aria-hidden="true">
        {#each hours as h (h)}<span>{h}</span>{/each}
      </div>
      {#each days as d (d)}
        <div class="col" class:today={d === today} role="group" aria-label="{DAYS[d]}요일{d === today ? ' (오늘)' : ''}">
          {#each byDay.get(d) ?? [] as s (s.name + s.start)}
            <button
              class="blk"
              class:one={s.periods.length === 1}
              style:--c={color(s)}
              style:--top={top(s)}
              style:--len={s.periods.length}
              onclick={(e) => pick(e, s)}
              aria-label="{DAYS[d]}요일 {s.start} {periodLabel(s)} {s.name}{s.room ? ` ${s.room}` : ''}"
            >
              <strong>{s.name}</strong>
              {#if s.room}<span>{s.room}</span>{/if}
            </button>
          {/each}
          {#if d === today && showNow}
            <i class="now-line" style:--top={(nowMin - range.from * 60) / 60} aria-hidden="true"></i>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{:else}
  <div class="empty"><strong>이번 학기 시간표가 없어요</strong></div>
{/if}

<Popover {anchor} bind:open={popOpen} placement="top" label="수업 정보">
  {#if picked}
    <div class="info" style:--c={color(picked)}>
      <strong>{picked.name}</strong>
      <span>{DAYS[picked.weekday]}요일 {picked.start} · {periodLabel(picked)}</span>
      {#if picked.room}<span>강의실 {picked.room}</span>{/if}
      {#if picked.code}<span class="code">{picked.code}</span>{/if}
    </div>
  {/if}
</Popover>

<style>
  .tt {
    --row: 64px;
    --time: 34px;
    width: 100%;
  }

  .tt.compact {
    --row: 58px;
    --time: 28px;
  }

  .head,
  .body {
    display: grid;
    grid-template-columns: var(--time) repeat(var(--cols), minmax(0, 1fr));
  }

  .head {
    position: sticky;
    top: 0;
    z-index: 2;
    padding-bottom: 2px;
    background: var(--surface);
  }

  .dh {
    display: grid;
    place-items: center;
    height: 28px;
    margin: 0 2px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 700;
    color: var(--text-3);
  }

  .dh.today {
    background: var(--primary-weak);
    color: var(--primary-text);
  }

  .body {
    height: calc(var(--rows) * var(--row));
    margin-top: 10px;
  }

  .hours {
    display: grid;
    grid-template-rows: repeat(var(--rows), var(--row));
  }

  .hours span {
    font-size: 11px;
    font-weight: 650;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    text-align: right;
    padding-right: 6px;
    transform: translateY(-7px);
  }

  .col {
    position: relative;
    border-left: 1px solid var(--border);
    background-image: linear-gradient(var(--border) 1px, transparent 1px);
    background-size: 100% var(--row);
  }

  .col:last-child {
    border-right: 1px solid var(--border);
  }

  .col.today {
    background-color: color-mix(in srgb, var(--primary) 4%, transparent);
  }

  .blk {
    position: absolute;
    left: 2px;
    right: 2px;
    top: calc(var(--top) * var(--row) + 2px);
    height: calc(var(--len) * var(--row) - 4px);
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px 6px;
    border-radius: 8px;
    overflow: hidden;
    text-align: left;
    background: color-mix(in srgb, var(--c) var(--timetable-tint), var(--timetable-surface));
    color: var(--timetable-text);
    transition: transform 0.1s;
  }

  .blk:active {
    transform: scale(0.98);
  }

  .blk strong {
    font-size: 12px;
    font-weight: 700;
    line-height: 1.3;
    letter-spacing: -0.02em;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
    overflow-wrap: anywhere;
  }

  .blk span {
    font-size: 11px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--timetable-text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .blk.one strong {
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  .compact .blk {
    padding: 4px 4px;
  }

  .compact .blk strong {
    font-size: 11px;
  }

  .compact .blk span {
    font-size: 10px;
  }

  .now-line {
    position: absolute;
    left: -1px;
    right: 0;
    top: calc(var(--top) * var(--row));
    height: 2px;
    background: var(--danger);
    z-index: 1;
    pointer-events: none;
  }

  .now-line::before {
    content: '';
    position: absolute;
    left: -4px;
    top: -3px;
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--danger);
  }

  .info {
    display: grid;
    gap: 2px;
    font-size: 13px;
    color: var(--text-2);
  }

  .info strong {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 14.5px;
    font-weight: 750;
    color: var(--text);
  }

  .info strong::before {
    content: '';
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 3px;
    background: var(--c);
  }

  .info .code {
    font-size: 12px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
</style>
