<script lang="ts">
  import type { CalendarStat } from './summary';

  let { narrow, stat, counts, onselect }: {
    narrow: boolean;
    stat: CalendarStat | null;
    counts: Record<CalendarStat, number | string>;
    onselect: (stat: CalendarStat) => void;
  } = $props();
</script>

<div class="summary" class:picker={narrow}>
    {#if narrow}
      <button class="stat" class:on={stat === 'all'} onclick={() => onselect('all')} aria-expanded={stat === 'all'}>
        <strong>{counts.all}</strong><span>전체</span>
      </button>
    {/if}
    <button class="stat" class:on={stat === 'week'} onclick={() => onselect('week')} aria-expanded={stat === 'week'}>
      <strong>{counts.week}</strong><span>7일 내 마감</span>
    </button>
    <button class="stat" class:on={stat === 'assign'} onclick={() => onselect('assign')} aria-expanded={stat === 'assign'}>
      <strong>{counts.assign}</strong><span>남은 과제</span>
    </button>
    <button class="stat" class:on={stat === 'vod'} onclick={() => onselect('vod')} aria-expanded={stat === 'vod'}>
      <strong>{counts.vod}</strong><span>남은 강의</span>
    </button>
    <button class="stat" class:on={stat === 'todo'} onclick={() => onselect('todo')} aria-expanded={stat === 'todo'}>
      <strong>{counts.todo}</strong><span>남은 할 일</span>
    </button>
    <button class="stat danger" class:on={stat === 'missed'} onclick={() => onselect('missed')} aria-expanded={stat === 'missed'}>
      <strong>{counts.missed}</strong><span>놓친 항목</span>
    </button>
  </div>

<style>
  .summary.picker { grid-template-columns: repeat(3,minmax(0,1fr)); margin-bottom: 16px; }
  .summary.picker .stat { padding-inline: 8px; }
  .summary {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 0;
    margin-bottom: 16px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .stat {
    display: grid;
    gap: 2px;
    padding: 10px 8px;
    border-radius: 12px;
    background: transparent;
    border: 1px solid transparent;
  }
  .stat strong {
    font-size: 24px;
    font-weight: 750;
    letter-spacing: -0.03em;
    font-variant-numeric: tabular-nums;
  }
  .stat span {
    font-size: 12px;
    color: var(--text-3);
    font-weight: 600;
  }
  .stat.danger strong {
    color: var(--danger);
  }
  .stat {
    text-align: left;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }
  .stat:hover {
    border-color: var(--border-strong);
  }
  .stat.on {
    border-color: transparent;
    background: var(--primary-weak);
    color: var(--primary-text);
  }
  @media (max-width: 639px) {
  .stat {
      padding: 10px 2px;
      text-align: center;
    }
  .stat strong {
      font-size: 19px;
    }
  .stat span {
      font-size: 11px;
      line-height: 1.3;
    }
  }
</style>
