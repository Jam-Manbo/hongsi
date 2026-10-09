<script lang="ts">
  import { clock } from '../../shared/state/clock.svelte';
  import { itemStatus, isFinished } from './colors';
  import { dday, dueDate, dueTime } from '../../shared/utils/format';
  import type { CalendarItem } from '../../shared/types';
  import Icon from '../../shared/ui/Icon.svelte';

  let {
    item,
    color,
    course,
    showDate = false,
    onopen,
    ontoggle,
  }: {
    item: CalendarItem;
    color: string;
    course: string;
    showDate?: boolean;
    onopen: (item: CalendarItem) => void;
    ontoggle: (item: CalendarItem) => void;
  } = $props();

  const finished = $derived(isFinished(item));
  const status = $derived(itemStatus(item));
  const d = $derived(item.due ? dday(item.due, clock.now) : null);
</script>

<div class="row" class:finished style:--c={color}>
  <button
    class="check"
    class:on={item.done}
    onclick={() => ontoggle(item)}
    aria-label={item.done ? '완료 체크 해제' : '완료로 체크'}
    aria-pressed={item.done}
  >
    {#if item.done}<Icon name="tick" size={15} stroke={2.6} />{/if}
  </button>
  <button class="main" onclick={() => onopen(item)}>
    <span class="copy">
      <span class="title">
        <span class="text"><span class="kind" role="img" aria-label={item.kind === 'vod' ? '온라인 강의' : '과제'}><Icon name={item.kind === 'vod' ? 'play' : 'file'} size={15} /></span>{item.title}</span>
      </span>
      <span class="course">{course}</span>
      <span class="deadline">
        {#if item.due && showDate}<span class="deadline-date">{dueDate(item.due)}</span>{' '}{/if}
        <span class="deadline-time">
          {#if item.due}<span>{dueTime(item.due)} 마감</span>{/if}
        </span>
      </span>
    </span>
    <span class="agenda-side">
      <span class="chip {status.tone}">{status.label}</span>
      {#if d && !finished}<span class="dday {d.tone}">{d.label}</span>{/if}
    </span>
  </button>
</div>

<style>
  .row {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px 12px 10px 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }

  .row::before {
    content: '';
    position: absolute;
    left: 0;
    top: 18px;
    bottom: 18px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--c);
  }

  .check { position: relative; flex: none; display: grid; place-items: center; width: 44px; height: 44px; border-radius: 12px; color: var(--surface); }
  .check::before { content: ''; position: absolute; width: 23px; height: 23px; border-radius: 8px; border: 1.5px solid var(--border-strong); transition: background 150ms, border-color 150ms; }
  .check.on::before { background: var(--ok); border-color: var(--ok); }
  .check :global(svg) { position: relative; z-index: 1; }
  .check:hover { background: var(--surface-2); }

  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 5px 6px;
    text-align: left;
    min-height: 44px;
    align-content: center;
  }

  .copy {
    min-width: 0;
    display: grid;
    gap: 5px;
  }

  .title {
    display: block;
    font-weight: 650;
    font-size: 14.5px;
    letter-spacing: -0.01em;
    overflow: hidden;
    white-space: normal;
    text-overflow: ellipsis;
  }

  .kind {
    display: inline-block;
    vertical-align: -2px;
    margin-right: 5px;
    color: var(--c);
  }

  .text { min-width: 0; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; line-height: 1.45; overflow-wrap: anywhere; }

  .finished .title {
    color: var(--text-3);
    text-decoration: line-through;
  }

  .course {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--text-3);
  }

  .deadline {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.5;
    font-variant-numeric: tabular-nums;
  }

  .deadline-date { white-space: nowrap; }
  .deadline-time { white-space: nowrap; }

  .dday {
    font-size: 12px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    color: var(--text-3);
  }

  .dday.today,
  .dday.soon {
    color: var(--danger);
  }

  .dday.past {
    color: var(--text-3);
  }
</style>
