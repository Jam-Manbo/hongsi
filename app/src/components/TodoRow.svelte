<script lang="ts">
  import { dday, shortDate, time } from '../lib/format';
  import { toggleTodo } from '../lib/todos.svelte';
  import type { Todo } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    todo,
    color,
    course,
    showDate = false,
    onopen,
  }: { todo: Todo; color: string; course: string; showDate?: boolean; onopen: (t: Todo) => void } = $props();

  const done = $derived(todo.doneAt !== null);
  const d = $derived(todo.dueAt && !done ? dday(todo.dueAt + (todo.allDay ? 86_399 : 0)) : null);
</script>

<div class="row" class:done style:--c={color}>
  <button class="check" class:on={done} onclick={() => toggleTodo(todo)} aria-label={done ? '할 일 완료 취소' : '할 일 완료'} aria-pressed={done}>
    {#if done}<Icon name="tick" size={15} stroke={2.6} />{/if}
  </button>
  <button class="main" onclick={() => onopen(todo)}>
    <span class="title"><span class="tag">할 일</span><span class="text">{todo.title}</span></span>
    <span class="meta">
      <span>{course}</span>
      {#if todo.dueAt}<span>· {showDate ? shortDate(todo.dueAt) : ''} {todo.allDay ? '하루 종일' : time(todo.dueAt)}</span>{/if}
      {#if todo.note}<span>· 메모</span>{/if}
    </span>
  </button>
  <div class="agenda-side">
    {#if d}<span class="dday {d.tone}">{d.label}</span>{/if}
  </div>
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
  .check.on::before { background: var(--c); border-color: var(--c); }
  .check :global(svg) { position: relative; z-index: 1; }
  .check:hover { background: var(--surface-2); }

  .main {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 5px;
    text-align: left;
    min-height: 44px;
    align-content: center;
  }

  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 650;
    font-size: 14.5px;
    overflow: hidden;
    white-space: normal;
    text-overflow: ellipsis;
  }

  .tag {
    flex: none;
    font-size: 11px;
    font-weight: 750;
    padding: 1px 6px;
    border-radius: 6px;
    color: var(--text-2);
    background: var(--surface-3);
  }

  .text { min-width: 0; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; line-height: 1.45; }

  .done .title {
    color: var(--text-3);
    text-decoration: line-through;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    font-size: 12px;
    color: var(--text-3);
    overflow: hidden;
    white-space: normal;
    text-overflow: ellipsis;
  }

  .dday {
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    font-weight: 800;
    color: var(--text-3);
  }

  .dday.today,
  .dday.soon {
    color: var(--danger);
  }
</style>
