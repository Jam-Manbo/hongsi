<script lang="ts">
  import { untrack } from 'svelte';
  import { isApp } from '../lib/api';
  import { settings } from '../lib/settings.svelte';
  import { calendar } from '../lib/store.svelte';
  import { courseLabel } from '../lib/semester';
  import { pendingTodos, removeTodo, saveTodo, fromUnix, toUnix } from '../lib/todos.svelte';
  import TimeWheel from './TimeWheel.svelte';
  import DateField from './DateField.svelte';
  import type { Course, Todo } from '../lib/types';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import DeadlineAlerts from './DeadlineAlerts.svelte';

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
  const parent = $derived(calendar.data?.items.find((item) => item.key === parentKey));
  const parentCompleted = $derived(todo === null && parent?.done === true);
  const linkedCourse = $derived(courses.find((course) => course.id === (parent?.courseId ?? courseId)));
  let notify = $state(true);
  let alertLeads = $state<number[] | null>(null);
  let busy = $state(false);
  const saving = $derived(busy || (todo !== null && pendingTodos.has(todo.id)));
  let discardOpen = $state(false);
  let deleteOpen = $state(false);
  let initialValues = $state('');
  const formValues = $derived(JSON.stringify({
    title, note, courseId, date, time, parentKey, notify,
    alertLeads: [...(alertLeads ?? settings.alertLeads)].sort((a, b) => b - a),
  }));
  let titleEl: HTMLInputElement | undefined = $state();
  let editing: number | 'new' | null = null;

  $effect(() => {
    const next = open ? (todo?.id ?? 'new') : null;
    if (editing === next) return;
    editing = next;
    untrack(() => {
      discardOpen = false;
      deleteOpen = false;
      if (!open) return;
      const due = todo?.due != null ? fromUnix(todo.due - (todo.allDay ? 86_400 : 0)) : null;
      title = todo?.title ?? '';
      note = todo?.note ?? '';
      courseId = todo ? todo.courseId : (draft.courseId ?? null);
      date = due?.date ?? draft.date ?? '';
      time = todo && !todo.allDay && due ? due.time : '';
      parentKey = todo ? todo.parentKey : (draft.parentKey ?? null);
      notify = todo?.notify ?? true;
      alertLeads = todo?.alertLeads ? [...todo.alertLeads] : null;
      initialValues = formValues;
      if (!todo) queueMicrotask(() => titleEl?.focus());
    });
  });

  $effect(() => {
    if (!date) time = '';
  });

  function canClose() {
    if (saving || deleteOpen) return false;
    if (formValues === initialValues) return true;
    discardOpen = true;
    return false;
  }

  function requestClose() {
    if (canClose()) open = false;
  }

  function discard() {
    discardOpen = false;
    open = false;
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (saving || deleteOpen || parentCompleted || !title.trim() || !date) return;
    busy = true;
    const ok = await saveTodo(todo?.id ?? null, {
      title: title.trim(),
      note: note.trim(),
      courseId: parentKey ? (parent?.courseId ?? courseId) : courseId,
      parentKey,
      due: toUnix(date, time) + (!time ? 86_400 : 0),
      allDay: !time,
      notify,
      alertLeads,
    });
    busy = false;
    if (ok) open = false;
  }

  function requestRemove() {
    if (!todo || saving || discardOpen) return;
    deleteOpen = true;
  }

  async function remove() {
    if (!deleteOpen || !todo || saving) return;
    busy = true;
    const ok = await removeTodo(todo);
    busy = false;
    if (ok) {
      deleteOpen = false;
      open = false;
    }
  }
</script>

<Sheet bind:open title={todo ? '할 일' : '할 일 추가'} onbeforeclose={canClose}>
  <form id="todo-form" class="form" onsubmit={submit}>
    {#if parentKey}
      <p class="linked"><Icon name="file" size={15} />{parent?.title ?? draft.parentTitle ?? '연결된 과제·강의'} 관련 할 일</p>
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
      {#if parentKey}
        <div class="chips">
          <span class="c on" style:--c={colors.get(parent?.courseId ?? courseId ?? 0) ?? 'var(--todo-neutral)'}>
            <i></i>{linkedCourse ? courseLabel(linkedCourse, settings.semesterDisplay) : '연결된 과목'}
          </span>
        </div>
      {:else}
        <div class="chips" role="radiogroup" aria-label="과목">
          <button type="button" class="c" class:on={courseId === null} style:--c="var(--todo-neutral)" onclick={() => (courseId = null)} role="radio" aria-checked={courseId === null}>
            <i></i>공통
          </button>
          {#each courses as c (c.id)}
            <button type="button" class="c" class:on={courseId === c.id} style:--c={colors.get(c.id)} onclick={() => (courseId = c.id)} role="radio" aria-checked={courseId === c.id}>
              <i></i>{courseLabel(c, settings.semesterDisplay)}
            </button>
          {/each}
        </div>
      {/if}
    </div>

    {#if isApp}
      <div class="field">
        <span>날짜</span>
        <DateField bind:value={date} required />
      </div>
      {#if date}
        <div class="field">
          <span>시간</span>
          <div class="seg" role="radiogroup" aria-label="시간">
            <button type="button" role="radio" aria-checked={!time} class:on={!time} onclick={() => (time = '')}>하루 종일</button>
            <button type="button" role="radio" aria-checked={!!time} class:on={!!time} onclick={() => (time = time || '09:00')}>시간 정하기</button>
          </div>
          {#if time}<TimeWheel bind:value={time} minuteStep={1} />{/if}
        </div>
      {/if}
    {:else}
      <div class="row">
        <div class="field">
          <span>날짜</span>
          <DateField bind:value={date} required />
        </div>
        <label class="field">
          <span>시간</span>
          <input type="time" bind:value={time} disabled={!date} />
        </label>
      </div>
    {/if}

    {#if date}
      <DeadlineAlerts enabled={notify} leads={alertLeads} label="이 할 일 마감 알림" disabled={busy}
        ontoggle={(on) => { notify = on; }} onchange={(leads) => { alertLeads = leads; }} />
    {/if}

    <label class="field">
      <span>메모</span>
      <textarea bind:value={note} rows="3" maxlength="2000" placeholder="자세한 내용"></textarea>
    </label>
  </form>

  {#snippet footer()}
    {#if todo}
      <button class="btn btn-danger w1" disabled={saving} onclick={requestRemove} aria-label="할 일 지우기"><Icon name="close" size={18} />지우기</button>
    {:else}
      <button class="btn btn-ghost w1" onclick={requestClose}>취소</button>
    {/if}
    <button class="btn btn-primary w2" form="todo-form" disabled={saving || parentCompleted || !title.trim() || !date}>{saving ? '처리 중…' : '저장'}</button>
  {/snippet}
</Sheet>

<Sheet bind:open={discardOpen} title="작성중인 내용을 버릴까요?" confirm showClose={false}>
  <p class="discard-description">저장하지 않은 변경사항이 있어요.</p>
  {#snippet footer()}
    <button class="btn btn-danger w1" onclick={discard}>삭제</button>
    <button class="btn btn-primary w1" onclick={() => (discardOpen = false)}>계속 작성</button>
  {/snippet}
</Sheet>

<Sheet bind:open={deleteOpen} title="할 일을 삭제할까요?" confirm showClose={false} onbeforeclose={() => !saving}>
  <div class="delete-description">
    <strong>{todo?.title}</strong>
    <p>삭제하면 되돌릴 수 없어요.</p>
  </div>
  {#snippet footer()}
    <button class="btn btn-ghost w1" disabled={saving} onclick={() => (deleteOpen = false)}>취소</button>
    <button class="btn btn-danger w1" disabled={saving} onclick={remove}>{saving ? '삭제 중…' : '삭제'}</button>
  {/snippet}
</Sheet>

<style>
  .delete-description { display: grid; gap: 8px; font-size: 14px; line-height: 1.65; }
  .delete-description strong { overflow-wrap: anywhere; }
  .delete-description p { margin: 0; color: var(--text-2); }
  .discard-description { font-size: 14px; line-height: 1.65; color: var(--text-2); }

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
