<script lang="ts">
  import { onMount } from 'svelte';
  import CourseFilters from '../features/calendar/CourseFilters.svelte';
  import CalendarSummary from '../features/calendar/CalendarSummary.svelte';
  import { createCalendarCollapse } from '../features/calendar/collapse.svelte';
  import type { CalendarStat } from '../features/calendar/summary';
  import { MediaQuery } from 'svelte/reactivity';
  import { isApp } from '../shared/api/api';
  import { agendaEntries, type AgendaEntry } from '../features/calendar/agenda';
  import { clock } from '../shared/state/clock.svelte';
  import { toggleDone } from '../features/calendar/actions.svelte';
  import { courseColors, isOverdue, isPending } from '../features/calendar/colors';
  import { ago, dueKey, dueTime, longDay, todayKey } from '../shared/utils/format';
  import { calendar } from '../features/calendar/calendar-resources.svelte';
  import { pref, setPref } from '../shared/state/preferences';
  import { todos } from '../features/calendar/todo-resource.svelte';
  import { refreshState, refreshTab } from '../shared/state/refresh.svelte';
  import { settings } from '../features/settings/settings.svelte';
  import { displayedTodos, isTodoPending, todoKey } from '../features/calendar/todos.svelte';
  import { focus, toast } from '../shared/state/ui.svelte';
  import type { AcademicTerm, CalendarItem, Todo } from '../shared/types';
  import TodoRow from '../features/calendar/TodoRow.svelte';
  import TodoSheet from '../features/calendar/TodoSheet.svelte';
  import Sheet from '../shared/ui/Sheet.svelte';
  import AgendaItem from '../features/calendar/AgendaItem.svelte';
  import Icon from '../shared/ui/Icon.svelte';
  import ItemSheet from '../features/classroom/ItemSheet.svelte';
  import LoadError from '../shared/ui/LoadError.svelte';
  import MonthCalendar from '../features/calendar/MonthCalendar.svelte';
  import Popover from '../shared/ui/Popover.svelte';
  import Skeleton from '../shared/ui/Skeleton.svelte';
  import EmptyState from '../shared/ui/EmptyState.svelte';

  type Filter = 'all' | 'assignment' | 'vod' | 'pending' | 'todo';
  const FILTERS: { id: Filter; label: string }[] = [
    { id: 'all', label: '전체' },
    { id: 'pending', label: '남은 일만' },
    { id: 'assignment', label: '과제' },
    { id: 'vod', label: '강의' },
    { id: 'todo', label: '할 일' },
  ];
  const COMMON = -1;

  const [y0, m0] = todayKey().split('-').map(Number);
  let year = $state(y0);
  let month = $state(m0);
  let selected = $state(todayKey());
  let filter = $state<Filter>('all');
  let hidden = $state(new Set<number>());
  let stat = $state<CalendarStat | null>(null);
  let detailKey = $state<string | null>(null);
  const detail = $derived(calendar.data?.items.find((i) => i.key === detailKey) ?? null);

  $effect(() => {
    const key = focus.item;
    if (!key || !calendar.data) return;
    if (calendar.data.items.some((i) => i.key === key)) detailKey = key;
    else toast('표시할 일정이 없어요.', 'info');
    focus.item = null;
  });

  onMount(() => {
    calendar.load();
    todos.load();
  });

  let todoOpen = $state(false);
  let editing = $state<Todo | null>(null);
  let draft = $state<{ courseId?: number | null; date?: string; parentKey?: string | null; parentTitle?: string }>({});

  function addTodo(extra: typeof draft = {}) {
    editing = null;
    draft = { date: selected, ...extra };
    todoOpen = true;
  }

  function editTodo(t: Todo) {
    summaryOpen = false;
    editing = t;
    draft = {};
    todoOpen = true;
  }

  $effect(() => {
    const id = focus.todo;
    if (id === null || !todos.data) return;
    const todo = displayedTodos().find((t) => t.id === id);
    if (todo) editTodo(todo);
    else toast('표시할 할 일이 없어요.', 'info');
    focus.todo = null;
  });

  const data = $derived(calendar.data);
  let selectedTerm = $state<string | null>(null);
  const terms = $derived.by(() => {
    const available = new Map<string, AcademicTerm | null>();
    for (const course of data?.courses ?? []) available.set(termKey(course.term), course.term);
    return [...available].map(([key, term]) => ({ key, label: termLabel(term), term })).sort((a, b) =>
      (b.term?.year ?? 0) - (a.term?.year ?? 0) || (b.term?.semester ?? 0) - (a.term?.semester ?? 0));
  });
  const showTerms = $derived(data?.semesterDisplay === 'all' && terms.length > 0);
  const activeTerm = $derived.by(() => {
    if (!showTerms) return 'all';
    return terms.find((term) => term.key === selectedTerm)?.key
      ?? terms.find((term) => data?.currentTerm && term.key === termKey(data.currentTerm))?.key
      ?? terms[0].key;
  });
  const termCourses = $derived((data?.courses ?? []).filter((course) => activeTerm === 'all' || termKey(course.term) === activeTerm));
  const termCourseIds = $derived(new Set([COMMON, ...termCourses.map((course) => course.id)]));
  const items = $derived((data?.items ?? []).filter((i) => (activeTerm === 'all' || termCourseIds.has(i.courseId)) && !hidden.has(i.courseId)
    && (settings.showUndatedAssignments || i.kind !== 'assignment' || i.due !== null)));
  const colors = $derived(courseColors(data?.courses ?? []));
  const courseName = (id: number) => data?.courses.find((c) => c.id === id)?.name ?? '';

  const visible = $derived(
    items.filter((i) => {
      if (filter === 'todo') return false;
      if (filter === 'assignment') return i.kind === 'assignment';
      if (filter === 'vod') return i.kind === 'vod';
      if (filter === 'pending') return isPending(i);
      return true;
    }),
  );

  const dayItems = $derived(visible.filter((i) => i.due !== null && dueKey(i.due) === selected));
  const courseTodos = $derived(displayedTodos().filter((t) => (activeTerm === 'all' || termCourseIds.has(t.courseId ?? COMMON)) && !hidden.has(t.courseId ?? COMMON)));
  const myTodos = $derived(
    courseTodos.filter((t) => {
      if (filter === 'assignment' || filter === 'vod') return false;
      if (filter === 'pending') return t.doneAt === null;
      return true;
    }),
  );
  const dayTodos = $derived(myTodos.filter((t) => todoKey(t) === selected));
  const dayEntries = $derived(agendaEntries(dayItems, dayTodos));
  const remainingTodos = $derived(courseTodos.filter((t) => t.doneAt === null));
  const weekTodos = $derived.by(() => {
    const now = Date.now() / 1000;
    return remainingTodos.filter((t) => isTodoPending(t, now) && t.due !== null && t.due - now < 7 * 86400);
  });
  const upcomingTodos = $derived(myTodos.filter((t) =>
    t.doneAt === null && t.due !== null && t.due * 1000 > clock.now));
  const todoColor = (t: Todo) => (t.courseId === null ? 'var(--todo-neutral)' : (colors.get(t.courseId) ?? 'var(--todo-neutral)'));
  const todoCourse = (t: Todo) => (t.courseId === null ? '공통' : courseName(t.courseId));
  const names = $derived(new Map((data?.courses ?? []).map((c) => [c.id, c.name])));
  const week = $derived.by(() => {
    const now = Date.now() / 1000;
    return items.filter((i) => isPending(i, now) && i.due !== null && i.due - now < 7 * 86400);
  });
  const upcoming = $derived(visible.filter((i) =>
    !i.done && i.due !== null && i.due * 1000 > clock.now));
  const upcomingEntries = $derived(agendaEntries(upcoming, upcomingTodos));
  const undatedEntries = $derived(agendaEntries(visible.filter((i) => i.due === null), []));

  const statItems = $derived.by(() => {
    const now = Date.now() / 1000;
    switch (stat) {
      case 'all':
        return items;
      case 'week':
        return week;
      case 'assign':
        return items.filter((i) => i.kind === 'assignment' && isPending(i, now));
      case 'vod':
        return items.filter((i) => i.kind === 'vod' && isPending(i, now));
      case 'missed':
        return items.filter((i) => isOverdue(i, now));
      default:
        return [];
    }
  });
  const statTodos = $derived(stat === 'all' ? courseTodos : stat === 'todo' ? remainingTodos : stat === 'week' ? weekTodos : []);
  const statNeedsTodos = $derived(stat === 'all' || stat === 'todo' || stat === 'week');
  const statLoading = $derived(statNeedsTodos && todos.data === null && !todos.error);
  const statEntries = $derived(agendaEntries(statItems, statTodos));
  const statHasItems = $derived(statEntries.length > 0);

  const courseIds = $derived([COMMON, ...(data?.courses ?? []).map((c) => c.id)]);
  const allCoursesSelected = $derived(courseIds.every((id) => !hidden.has(id)));
  const selectedCourseCount = $derived([...termCourseIds].filter((id) => !hidden.has(id)).length);
  const courseFilterActive = $derived(activeTerm !== 'all' || !allCoursesSelected);

  function termKey(term: AcademicTerm | null): string {
    return term ? `${term.year}-${term.semester}` : 'other';
  }

  function termLabel(term: AcademicTerm | null): string {
    if (!term) return '기타';
    const labels: Record<number, string> = { 10: '1학기', 11: '여름학기', 20: '2학기', 21: '겨울학기' };
    return `${term.year}년 ${labels[term.semester] ?? term.semester}`;
  }

  const narrow = new MediaQuery('max-width: 767px');
  let courseOpen = $state(false);
  let summaryOpen = $state(false);

  const summaryCounts = $derived({
    all: todos.data ? items.length + courseTodos.length : todos.error ? '—' : '…',
    week: todos.data ? week.length + weekTodos.length : todos.error ? '—' : '…',
    assign: items.filter((i) => i.kind === 'assignment' && isPending(i)).length,
    vod: items.filter((i) => i.kind === 'vod' && isPending(i)).length,
    todo: todos.data ? remainingTodos.length : todos.error ? '—' : '…',
    missed: items.filter((i) => isOverdue(i)).length,
  });

  function selectStat(next: CalendarStat) {
    stat = !narrow.current && stat === next ? null : next;
  }

  function openItem(item: CalendarItem) {
    summaryOpen = false;
    detailKey = item.key;
  }

  const sideable = new MediaQuery('min-width: 1200px');
  type Layout = 'wide' | 'side';
  let layout = $state<Layout>(pref<Layout>('calendar-layout', isApp ? 'wide' : 'side'));
  const side = $derived(sideable.current && layout === 'side');
  let dayPop = $state(false);
  let dayAnchor = $state<HTMLElement | null>(null);

  function setLayout(v: Layout) {
    layout = v;
    setPref('calendar-layout', v);
    dayPop = false;
  }

  function onPick(_key: string, el: HTMLElement) {
    if (narrow.current || side) return;
    if (dayPop && dayAnchor === el) {
      dayPop = false;
      return;
    }
    dayAnchor = el;
    dayPop = true;
  }

  function openFromPop(entry: AgendaEntry) {
    dayPop = false;
    if (entry.kind === 'todo') editTodo(entry.value);
    else openItem(entry.value);
  }

  const dock = createCalendarCollapse(() => narrow.current);

</script>

<div class="page">
  {#if !narrow.current}
    <div class="toolbar">
      <div class="filters" role="toolbar" aria-label="일정 필터">
        {#each FILTERS as f (f.id)}
          <button class="filter" aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
        {/each}
      </div>
      {#if sideable.current}
        <span class="seg layout-seg" role="group" aria-label="캘린더 배치">
          <button aria-pressed={layout === 'wide'} onclick={() => setLayout('wide')}>
            <Icon name="layout-below" size={16} stroke={2} />넓게
          </button>
          <button aria-pressed={layout === 'side'} onclick={() => setLayout('side')}>
            <Icon name="layout-side" size={16} stroke={2} />나란히
          </button>
        </span>
      {/if}
      {#if !isApp}
      <button
        class="icon-btn refresh"
        onclick={() => refreshTab('calendar')}
        disabled={refreshState.calendar.busy}
        aria-label="새로고침"
        title={calendar.at ? `${ago(calendar.at / 1000)} 업데이트` : '새로고침'}
      >
        <span class:spin={refreshState.calendar.busy}><Icon name="refresh" size={19} /></span>
      </button>
      {/if}
    </div>
  {/if}

  <LoadError resource={calendar} what="캘린더를" />
  <LoadError resource={todos} what="할 일을" />

  {#if !data}
    {#if !calendar.error}
      <Skeleton rows={1} height={380} />
      <div style="height: 12px"></div>
      <Skeleton rows={3} height={64} />
    {/if}
  {:else}
    {@render summaryControls()}
    {#if !narrow.current}
      {@render statResults()}
      <CourseFilters {terms} {showTerms} {activeTerm} {termCourses} {termCourseIds} {colors} bind:hidden bind:selectedTerm />
    {/if}

    <div class="board calendar-board" data-layout={side ? 'side' : 'wide'}>
      <div class="cal-wrap" bind:this={dock.calWrap} style:--collapse={dock.collapse} style:--calendar-height="{dock.monthH - (dock.monthH - dock.stripH) * dock.collapse}px">
        <div class="calendar-dock">
          <div class="month-view" bind:this={dock.monthView} inert={narrow.current && dock.compactInteractive}>
            <MonthCalendar
              collapse={dock.collapse}
              bind:year
              bind:month
              bind:selected
              items={visible}
              {colors}
              {names}
              todos={myTodos}
              onpick={onPick}
            />
          </div>
          {#if narrow.current}
            <div class="strip" class:on={dock.stripOn} inert={!dock.compactInteractive} bind:this={dock.weekView}>
              <MonthCalendar
                week
                bind:year
                bind:month
                bind:selected
                items={visible}
                {colors}
                {names}
                todos={myTodos}
                onpick={dock.afterStripPick}
                onexpand={dock.expand}
              />
            </div>
          {/if}
        </div>
      </div>
      {#if narrow.current}<div class="calendar-space" bind:this={dock.calSpace} style:height="{dock.monthH}px" aria-hidden="true"></div>{/if}

      <div class="lists" style:--strip-h="{narrow.current ? dock.stripH : 0}px">
        {#if narrow.current}
          <div class="list-controls">
            <div class="filters" role="toolbar" aria-label="일정 필터">
              {#each FILTERS as f (f.id)}
                <button class="filter" aria-pressed={filter === f.id} onclick={() => (filter = f.id)}>{f.label}</button>
              {/each}
            </div>
            {#if !isApp}<button class="icon-btn refresh" onclick={() => refreshTab('calendar')} disabled={refreshState.calendar.busy} aria-label="새로고침"><span class:spin={refreshState.calendar.busy}><Icon name="refresh" size={19} /></span></button>{/if}
          </div>
        {/if}
        <section class="agenda">
          <h2 class="day-title" bind:this={dock.dayTitle}>
            {longDay(selected)}
            {#if selected === todayKey()}<span class="chip primary">오늘</span>{/if}
            <button class="add" onclick={() => addTodo()}><Icon name="plus" size={16} stroke={2.4} />할 일</button>
          </h2>
          {#if dayEntries.length}
            <div class="list">
              {#each dayEntries as entry (entry.key)}{@render agendaRow(entry)}{/each}
            </div>
          {:else}
            <EmptyState message="이날 마감인 일정이 없어요." />
          {/if}
        </section>

        <section class="upcoming">
          <h2 class="section-title up-title">다가오는 일정</h2>
          {#if upcomingEntries.length}
            <div class="list">
              {#each upcomingEntries as entry (entry.key)}
                {@render agendaRow(entry, true)}
              {/each}
            </div>
          {:else}
            <EmptyState message="다가오는 일정이 없어요." />
          {/if}
        </section>
        {#if undatedEntries.length}
          <section class="undated">
            <h2 class="section-title">날짜 미정</h2>
            <div class="list">
              {#each undatedEntries as entry (entry.key)}
                {@render agendaRow(entry)}
              {/each}
            </div>
          </section>
        {/if}
      </div>
    </div>

    <Popover anchor={dayAnchor} bind:open={dayPop} placement="side" label="{longDay(selected)} 일정">
      <div class="pop-day">
        <header>
          <strong>{longDay(selected)}</strong>
          <button class="icon-btn pop-close" onclick={() => (dayPop = false)} aria-label="닫기"><Icon name="close" size={17} /></button>
        </header>
        {#if dayEntries.length}
          <ul>
            {#each dayEntries as entry (entry.key)}
              <li>
                <button
                  class="pop-row"
                  class:finished={entry.done}
                  style:--c={entry.kind === 'todo' ? todoColor(entry.value) : (colors.get(entry.value.courseId) ?? 'var(--text-3)')}
                  onclick={() => openFromPop(entry)}
                >
                  <i class="shape" class:todo={entry.kind === 'todo'} class:vod={entry.kind === 'item' && entry.value.kind === 'vod'} aria-hidden="true"></i>
                  <span class="pt">{entry.value.title}</span>
                  {#if entry.kind === 'todo'}
                    <span class="ptime">{entry.value.allDay || entry.value.due === null ? '하루 종일' : dueTime(entry.value.due)}</span>
                  {:else if entry.value.due !== null}
                    <span class="ptime">{dueTime(entry.value.due)}</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <EmptyState message="이날 마감인 일정이 없어요." compact />
        {/if}
        <button
          class="btn btn-soft pop-add"
          onclick={() => {
            dayPop = false;
            addTodo();
          }}><Icon name="plus" size={16} stroke={2.4} />이날 할 일 추가</button
        >
      </div>
    </Popover>
  {/if}
</div>

{#snippet agendaRow(entry: AgendaEntry, showDate = false)}
  {#if entry.kind === 'todo'}
    <TodoRow todo={entry.value} {showDate} color={todoColor(entry.value)} course={todoCourse(entry.value)} onopen={editTodo} />
  {:else}
    <AgendaItem
      item={entry.value}
      {showDate}
      color={colors.get(entry.value.courseId) ?? 'var(--text-3)'}
      course={courseName(entry.value.courseId)}
      onopen={openItem}
      ontoggle={toggleDone}
    />
  {/if}
{/snippet}

{#snippet entryList(entries: AgendaEntry[], showDate = false)}
  {@const pending = entries.filter((entry) => !entry.done)}
  {@const completed = entries.filter((entry) => entry.done)}
  {#if pending.length}
    <div class="list">
      {#each pending as entry (entry.key)}{@render agendaRow(entry, showDate)}{/each}
    </div>
  {/if}
  {#if completed.length}
    <details class="completed-list">
      <summary><span class="completed-arrow"><Icon name="right" size={18} stroke={2.6} /></span><span>완료한 일정 {completed.length}개</span></summary>
      <div class="list">
        {#each completed as entry (entry.key)}{@render agendaRow(entry, showDate)}{/each}
      </div>
    </details>
  {/if}
{/snippet}

{#snippet summaryControls()}
  {#if narrow.current}
    <div class="mobile-controls">
      <button class="summary-trigger" onclick={() => { stat = 'week'; summaryOpen = true; }} aria-haspopup="dialog">
        <span>7일 내 마감</span><strong>{todos.data ? week.length + weekTodos.length : todos.error ? '—' : '…'}</strong><Icon name="down" size={14} />
      </button>
      <button class="course-trigger" class:active={courseFilterActive} onclick={() => (courseOpen = true)} aria-haspopup="dialog">
        과목 필터{#if courseFilterActive}<span class="count">{selectedCourseCount}/{courseIds.length}</span>{/if}<Icon name="down" size={14} />
      </button>
    </div>
  {:else}
    <CalendarSummary narrow={narrow.current} {stat} counts={summaryCounts} onselect={selectStat} />
  {/if}
{/snippet}



{#snippet statResults()}
  {#if stat}
    <section class="stat-list" data-kind={stat} class:card={statHasItems || statLoading}>
      {#if statLoading}
        <Skeleton rows={1} height={64} />
      {:else if statHasItems}
        {@render entryList(statEntries, true)}
      {:else if !statNeedsTodos || todos.data !== null}
        <EmptyState message={{ all: '표시할 일정이 없어요.', week: '7일 안에 마감할 일이 없어요.', assign: '남은 과제가 없어요.', vod: '남은 강의가 없어요.', todo: '남은 할 일이 없어요.', missed: '놓친 항목이 없어요.' }[stat]} />
      {/if}
    </section>
  {/if}
{/snippet}



{#if narrow.current}
  <Sheet bind:open={courseOpen} title="과목 필터">
    <CourseFilters {terms} {showTerms} {activeTerm} {termCourses} {termCourseIds} {colors} bind:hidden bind:selectedTerm picker />
    {#snippet footer()}<button class="btn btn-primary w1" onclick={() => (courseOpen = false)}>확인</button>{/snippet}
  </Sheet>
  <Sheet bind:open={summaryOpen} title="일정 요약">
    <div class="summary-picker"><CalendarSummary narrow={narrow.current} {stat} counts={summaryCounts} onselect={selectStat} />{@render statResults()}</div>
  </Sheet>
{/if}

<TodoSheet bind:open={todoOpen} todo={editing} {draft} courses={data?.courses ?? []} {colors} />

<ItemSheet
  item={detail}
  subtodos={detail ? displayedTodos().filter((t) => t.parentKey === detail.key) : []}
  onaddtodo={(i) =>
    addTodo({
      courseId: i.courseId,
      parentKey: i.key,
      parentTitle: i.title,
      date: i.due ? dueKey(i.due) : selected,
    })}
  oneditodo={editTodo}
  onclose={() => (detailKey = null)}
  course={detail ? courseName(detail.courseId) : ''}
  color={detail ? (colors.get(detail.courseId) ?? 'var(--text-3)') : ''}
  ontoggle={toggleDone}
/>

<style>
  .completed-list { margin-top: 12px; }
  .completed-list summary { display: flex; align-items: center; gap: 8px; min-height: 44px; padding: 10px 0; cursor: pointer; list-style: none; font-size: 15px; font-weight: 700; color: var(--text); }
  .completed-list summary::-webkit-details-marker { display: none; }
  .completed-list summary::marker { content: ''; }
  .completed-arrow { display: flex; flex: none; transition: transform 150ms; }
  .completed-list[open] > summary .completed-arrow { transform: rotate(90deg); }
  .mobile-controls { display: flex; justify-content: space-between; align-items: center; gap: 8px; margin-bottom: 12px; }
  .summary-trigger { display: inline-flex; align-items: center; gap: 7px; min-height: 40px; padding: 0 2px; color: var(--text-2); font-size: 13px; font-weight: 650; }
  .summary-trigger strong { font-size: 19px; font-variant-numeric: tabular-nums; color: var(--primary-text); }
  .course-trigger { display: inline-flex; align-items: center; gap: 6px; min-height: 40px; padding: 0 12px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); font-size: 12px; font-weight: 650; white-space: nowrap; }
  .course-trigger.active { color: var(--primary-text); border-color: var(--primary); background: var(--primary-weak); }
  .course-trigger .count { font-variant-numeric: tabular-nums; font-size: 11px; }
  .list-controls { display: flex; gap: 6px; align-items: center; margin-bottom: 12px; min-width: 0; }
  .list-controls .filters { flex: 1; min-width: 0; }
  .list-controls .refresh { width: 36px; height: 40px; }
  .list-controls .filter { min-height: 40px; padding-inline: 10px; }
  .summary-picker .stat-list.card { padding: 0; border: 0; background: transparent; box-shadow: none; }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
  }
  .toolbar .filters {
    flex: 1;
  }
  .refresh {
    flex: none;
  }
  .layout-seg {
    flex: none;
  }
  .layout-seg button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 12px 0 10px;
  }
  .stat-list {
    margin-bottom: 12px;
    display: grid;
    gap: 8px;
  }
  .stat-list.card { padding: 12px; }
  .add {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-height: 40px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--primary-weak);
    color: var(--primary-text);
    font-size: 13px;
    font-weight: 700;
  }
  .board, .cal-wrap, .lists { min-width: 0; }
  .board[data-layout='wide'] { display: flex; flex-direction: column; width: 100%; }
  .board[data-layout='wide'] > .cal-wrap,
  .board[data-layout='wide'] > .lists { width: 100%; }
  .cal-wrap {
    scroll-margin-top: calc(var(--topbar-h, 64px) + 8px);
  }
  .calendar-space { flex: none; scroll-margin-top: calc(var(--topbar-h, 64px) + 8px); overflow-anchor: none; }
  @media (max-width: 767px) {
  .cal-wrap {
      position: sticky;
      top: var(--topbar-h, 64px);
      z-index: 15;
      height: 0;
      pointer-events: none;
    }
  .calendar-dock {
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      height: var(--calendar-height);
      overflow: hidden;
      border-radius: var(--radius);
      background: var(--surface);
      pointer-events: auto;
      overflow-anchor: none;
    }
  .month-view { opacity: min(1, max(0, calc((1 - var(--collapse)) / .15))); }
  .strip {
      position: absolute;
      top: 0;
      left: 0;
      width: 100%;
      opacity: max(0, calc((var(--collapse) - .85) / .15));
      transform: translate3d(0, calc((1 - var(--collapse)) * 20px), 0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
  .month-view { opacity: 1; }
  .strip { transform: none; opacity: 0; }
  .strip.on { opacity: 1; }
  }
  .lists {
    display: grid;
    align-content: start;
    margin-top: 16px;
  }
  @media (max-width: 767px) {
  .lists {
      min-height: calc(100dvh - var(--topbar-h, 64px) - var(--tabbar-h));
    }
  }
  .day-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    font-weight: 750;
    letter-spacing: -0.02em;
    margin: 4px 4px 10px;
    scroll-margin-top: calc(var(--topbar-h, 64px) + var(--strip-h, 0px) + 8px);
  }
  .list {
    display: grid;
    gap: 8px;
  }
  .pop-day {
    width: 280px;
    max-width: calc(100vw - 48px);
    display: grid;
    gap: 8px;
  }
  .pop-day header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin: -4px -6px 0 2px;
    font-size: 14.5px;
  }
  .pop-close {
    width: 32px;
    height: 32px;
  }
  .pop-day ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    max-height: 300px;
    overflow-y: auto;
  }
  .pop-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 8px;
    border-radius: 10px;
    text-align: left;
    transition: background 0.12s;
  }
  .pop-row:hover {
    background: var(--surface-2);
  }
  .shape {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--c);
  }
  .shape.vod {
    background: transparent;
    box-shadow: inset 0 0 0 2px var(--c);
  }
  .shape.todo {
    border-radius: 2px;
  }
  .pt {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ptime {
    flex: none;
    font-size: 12px;
    font-weight: 650;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .pop-row.finished .pt {
    color: var(--text-3);
    text-decoration: line-through;
  }
  .pop-add {
    min-height: 38px;
    font-size: 13.5px;
  }
  @media (min-width: 1024px) {
  .lists {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
      gap: 0 24px;
      align-items: start;
      margin-top: 20px;
    }
  .undated { grid-column: 1 / -1; }
  .up-title {
      margin: 4px 4px 10px;
      min-height: 32px;
      font-size: 16px;
      font-weight: 750;
      color: var(--text);
      letter-spacing: -0.02em;
    }
  .day-title {
      min-height: 32px;
    }
  }
  .board[data-layout='side'] {
    display: grid;
    grid-template-columns: minmax(0, 1fr) clamp(320px, 32%, 380px);
    gap: 24px;
    align-items: start;
  }
  .board[data-layout='side'] .lists {
    grid-template-columns: minmax(0, 1fr);
    gap: 22px;
    margin-top: 0;
  }
  .board[data-layout='side'] .day-title {
    margin-top: 2px;
  }
</style>
