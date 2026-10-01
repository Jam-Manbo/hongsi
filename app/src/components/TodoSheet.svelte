<script lang="ts">
  import { isApp } from '../lib/api';
  import { notificationsAllowed } from '../lib/notify';
  import { leadSummary } from '../lib/settings.svelte';
  import { removeTodo, saveTodo, fromUnix, toUnix } from '../lib/todos.svelte';
  import TimeWheel from './TimeWheel.svelte';
  import DateField from './DateField.svelte';
  import type { Course, Todo } from '../lib/types';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import Switch from './Switch.svelte';

  let {
    open = $bindable(false),
    todo = null,
    draft = {},
    courses,
    colors,
  }: {
    open: boolean;
    todo?: Todo | null;
    draft?: { courseId?: number | null; date?: string; parentKey?: string | null; parentTitle?: string };
    courses: Course[];
    colors: Map<number, string>;
  } = $props();

  let title = $state('');
  let note = $state('');
  let courseId = $state<number | null>(null);
  let date = $state('');
  let time = $state('');
  let parentKey = $state<string | null>(null);
  let notify = $state(true);
  let busy = $state(false);
  let titleEl: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (!open) return;
    const due = todo?.dueAt ? fromUnix(todo.dueAt) : null;
    title = todo?.title ?? '';
    note = todo?.note ?? '';
    courseId = todo ? todo.courseId : (draft.courseId ?? null);
    date = due?.date ?? draft.date ?? '';
    time = todo && !todo.allDay && due ? due.time : '';
    parentKey = todo ? todo.parentKey : (draft.parentKey ?? null);
    notify = todo?.notify ?? true;
    if (!todo) queueMicrotask(() => titleEl?.focus());
  });

  $effect(() => {
    if (!date) time = '';
  });

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    busy = true;
    if (date && notify) void notificationsAllowed(true);
    const ok = await saveTodo(todo?.id ?? null, {
      title: title.trim(),
      note: note.trim(),
      courseId,
      parentKey,
      dueAt: date ? toUnix(date, time) : null,
      allDay: !time,
      notify,
    });
    busy = false;
    if (ok) open = false;
  }

  async function remove() {
    if (!todo) return;
    open = false;
    await removeTodo(todo);
  }
</script>

<Sheet bind:open title={todo ? '할 일' : '할 일 추가'}>
  <form id="todo-form" class="form" onsubmit={submit}>
    {#if parentKey && draft.parentTitle && !todo}
      <p class="linked"><Icon name="file" size={15} />{draft.parentTitle}에 딸린 할 일</p>
    {/if}
    <input
      class="title"
      bind:this={titleEl}
      bind:value={title}
      maxlength="200"
      placeholder="예: 3장 복습, 실습 코드 정리"
      aria-label="할 일"
      required
    />

    <div class="field">
      <span>분류</span>
      <div class="chips" role="radiogroup" aria-label="과목">
        <button type="button" class="c" class:on={courseId === null} style:--c="var(--todo-neutral)" onclick={() => (courseId = null)} role="radio" aria-checked={courseId === null}>
          <i></i>공통
        </button>
        {#each courses as c (c.id)}
          <button type="button" class="c" class:on={courseId === c.id} style:--c={colors.get(c.id)} onclick={() => (courseId = c.id)} role="radio" aria-checked={courseId === c.id}>
            <i></i>{c.name}
          </button>
        {/each}
      </div>
    </div>

    {#if isApp}
      <div class="field">
        <span>날짜</span>
        <DateField bind:value={date} />
      </div>
      {#if date}
        <div class="field">
          <span>시간</span>
          <div class="seg" role="radiogroup" aria-label="시간">
            <button type="button" role="radio" aria-checked={!time} class:on={!time} onclick={() => (time = '')}>하루 종일</button>
            <button type="button" role="radio" aria-checked={!!time} class:on={!!time} onclick={() => (time = time || '09:00')}>시간 정하기</button>
          </div>
          {#if time}<TimeWheel bind:value={time} />{/if}
        </div>
      {/if}
    {:else}
      <div class="row">
        <div class="field">
          <span>날짜</span>
          <DateField bind:value={date} />
        </div>
        <label class="field">
          <span>시간</span>
          <input type="time" bind:value={time} disabled={!date} />
        </label>
      </div>
    {/if}

    {#if date}
      <div class="alert-field">
        <span class="label">마감 알림</span>
        {#if notify}<span class="lead">{leadSummary() || '알림 시간 없음'}</span>{/if}
        <Switch checked={notify} label="이 할 일 마감 알림" onchange={(v) => (notify = v)} />
      </div>
    {/if}

    <label class="field">
      <span>메모</span>
      <textarea bind:value={note} rows="3" maxlength="2000" placeholder="자세한 내용"></textarea>
    </label>
  </form>

  {#snippet footer()}
    {#if todo}
      <button class="btn btn-danger w1" onclick={remove} aria-label="할 일 지우기"><Icon name="close" size={18} />지우기</button>
    {:else}
      <button class="btn btn-ghost w1" onclick={() => (open = false)}>취소</button>
    {/if}
    <button class="btn btn-primary w2" form="todo-form" disabled={busy || !title.trim()}>{busy ? '저장 중…' : '저장'}</button>
  {/snippet}
</Sheet>

<style>
  .form {
    display: grid;
    gap: 16px;
  }

  .linked {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--primary-text);
  }

  .title {
    height: 52px;
    padding: 0 14px;
    border-radius: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    font-size: 17px;
    font-weight: 650;
  }

  .field {
    display: grid;
    gap: 8px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .row {
    display: grid;
    grid-template-columns: 1fr;
    gap: 16px 10px;
  }

  @media (min-width: 480px) {
    .row {
      grid-template-columns: 1.4fr 1fr;
    }
  }

  input[type='time'],
  textarea {
    height: 46px;
    padding: 0 12px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    font-size: 15px;
    font-weight: 550;
  }

  textarea {
    height: auto;
    padding: 10px 12px;
    resize: vertical;
    font-weight: 500;
  }

  input:focus,
  textarea:focus {
    outline: none;
    border-color: var(--primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .c {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .c i {
    width: 9px;
    height: 9px;
    border-radius: 3px;
    background: var(--c);
  }

  .c.on {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, var(--surface));
    color: var(--text);
    box-shadow: inset 0 0 0 1px var(--c);
  }

  .alert-field {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 6px 4px 6px 14px;
    border-radius: 12px;
    background: var(--surface-2);
    font-size: 13.5px;
    font-weight: 650;
    color: var(--text-2);
  }

  .alert-field .label {
    margin-right: auto;
  }

  .alert-field .lead {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-3);
  }

  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-3);
  }

  .seg button {
    height: 36px;
    border-radius: 9px;
    font-size: 13.5px;
    font-weight: 650;
    color: var(--text-2);
  }

  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-sm);
  }
</style>
