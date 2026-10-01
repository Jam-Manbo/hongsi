<script lang="ts">
  import { onMount } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { isApp } from '../lib/api';
  import { toggleDone } from '../lib/actions.svelte';
  import { courseColors, isFinished, isOverdue, isPending } from '../lib/colors';
  import { ago, dueKey, dueTime, longDay, time, todayKey } from '../lib/format';
  import { calendar, pref, setPref, todos } from '../lib/store.svelte';
  import { refreshState, refreshTab } from '../lib/refresh.svelte';
  import { settings } from '../lib/settings.svelte';
  import { isTodoPending, todoDeadline, todoKey } from '../lib/todos.svelte';
  import { focus, toast } from '../lib/ui.svelte';
  import type { CalendarItem, Todo } from '../lib/types';
  import TodoRow from '../components/TodoRow.svelte';
  import TodoSheet from '../components/TodoSheet.svelte';
  import AgendaItem from '../components/AgendaItem.svelte';
  import Icon from '../components/Icon.svelte';
  import ItemSheet from '../components/ItemSheet.svelte';
  import LoadError from '../components/LoadError.svelte';
  import MonthCalendar from '../components/MonthCalendar.svelte';
  import Popover from '../components/Popover.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import EmptyState from '../components/EmptyState.svelte';

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
  type Stat = 'week' | 'assign' | 'vod' | 'todo' | 'missed';
  let stat = $state<Stat | null>(null);
  let detailKey = $state<string | null>(null);
  const detail = $derived(calendar.data?.items.find((i) => i.key === detailKey) ?? null);

  $effect(() => {
    const key = focus.item;
    if (!key || !calendar.data) return;
    if (calendar.data.items.some((i) => i.key === key)) detailKey = key;
    else toast('이 일정은 삭제됐거나 이번 학기 목록에 없어요.', 'info');
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
    editing = t;
    draft = {};
    todoOpen = true;
  }

  $effect(() => {
    const id = focus.todo;
    if (id === null || !todos.data) return;
    const todo = todos.data.find((t) => t.id === id);
    if (todo) editTodo(todo);
    else toast('이 할 일은 삭제됐거나 보관 기간이 지났어요.', 'info');
    focus.todo = null;
  });

  const data = $derived(calendar.data);
  const items = $derived((data?.items ?? []).filter((i) => !hidden.has(i.courseId)
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
  const courseTodos = $derived((todos.data ?? []).filter((t) => !hidden.has(t.courseId ?? COMMON)));
  const myTodos = $derived(
    courseTodos.filter((t) => {
      if (filter === 'assignment' || filter === 'vod') return false;
      if (filter === 'pending') return t.doneAt === null;
      return true;
    }),
  );
  const dayTodos = $derived(myTodos.filter((t) => todoKey(t) === selected));
  const remainingTodos = $derived(courseTodos.filter((t) => t.doneAt === null)
    .sort((a, b) => (todoDeadline(a) ?? Infinity) - (todoDeadline(b) ?? Infinity) || a.id - b.id));
  const weekTodos = $derived.by(() => {
    const now = Date.now() / 1000;
    return remainingTodos.filter((t) => isTodoPending(t, now) && todoDeadline(t) !== null && todoDeadline(t)! - now < 7 * 86400);
  });
  const upcomingTodos = $derived(
    myTodos
      .filter((t) => {
        const key = todoKey(t);
        return isTodoPending(t) && (key === null || (key > todayKey() && key !== selected));
      })
      .slice(0, 8),
  );
  const todoColor = (t: Todo) => (t.courseId === null ? 'var(--todo-neutral)' : (colors.get(t.courseId) ?? 'var(--todo-neutral)'));
  const todoCourse = (t: Todo) => (t.courseId === null ? '공통' : courseName(t.courseId));
  const names = $derived(new Map((data?.courses ?? []).map((c) => [c.id, c.name])));
  const week = $derived.by(() => {
    const now = Date.now() / 1000;
    return items.filter((i) => isPending(i, now) && i.due !== null && i.due - now < 7 * 86400);
  });
  const upcoming = $derived(
    visible
      .filter((i) => {
        if (i.due === null || !isPending(i)) return false;
        const key = dueKey(i.due);
        return key > todayKey() && key !== selected;
      })
      .slice(0, 8),
  );
  const undated = $derived(visible.filter((i) => i.due === null));

  const statItems = $derived.by(() => {
    const now = Date.now() / 1000;
    switch (stat) {
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
  const statTodos = $derived(stat === 'todo' ? remainingTodos : stat === 'week' ? weekTodos : []);
  const statNeedsTodos = $derived(stat === 'todo' || stat === 'week');
  const statLoading = $derived(statNeedsTodos && todos.data === null && !todos.error);
  const statHasItems = $derived(statItems.length > 0 || statTodos.length > 0);

  function toggleCourse(id: number) {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    hidden = next;
  }

  const courseIds = $derived([COMMON, ...(data?.courses ?? []).map((c) => c.id)]);
  const allCoursesSelected = $derived(courseIds.every((id) => !hidden.has(id)));
  function toggleAllCourses() {
    hidden = allCoursesSelected ? new Set(courseIds) : new Set<number>();
  }

  const narrow = new MediaQuery('max-width: 767px');
  const sideable = new MediaQuery('min-width: 1200px');
  type Layout = 'wide' | 'side';
  let layout = $state<Layout>(pref<Layout>('calendar-layout', 'wide'));
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

  function openFromPop(key: string) {
    dayPop = false;
    detailKey = key;
  }

  let calWrap: HTMLDivElement | undefined = $state();
  let dayTitle: HTMLHeadingElement | undefined = $state();
  let stripOn = $state(false);
  let stripH = $state(0);

  $effect(() => {
    if (!narrow.current || !calWrap) {
      stripOn = false;
      return;
    }
    const wrap = calWrap;
    const scroller = wrap.closest('.scroller');
    if (!scroller) return;
    let raf = 0;
    const check = () => {
      raf = 0;
      const bar = document.querySelector('.topbar');
      const edge = (bar ?? scroller).getBoundingClientRect()[bar ? 'bottom' : 'top'];
      stripOn = wrap.getBoundingClientRect().bottom <= edge + 1;
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(check);
    };
    scroller.addEventListener('scroll', onScroll, { passive: true });
    check();
    return () => {
      scroller.removeEventListener('scroll', onScroll);
      cancelAnimationFrame(raf);
    };
  });

  function afterStripPick() {
    requestAnimationFrame(() => dayTitle?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
  }

  function expand() {
    calWrap?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }

</script>

<div class="page">
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

  <LoadError resource={calendar} what="캘린더를" />
  <LoadError resource={todos} what="할 일을" />

  {#if !data}
    {#if !calendar.error}
      <Skeleton rows={1} height={380} />
      <div style="height: 12px"></div>
      <Skeleton rows={3} height={64} />
    {/if}
  {:else}
    <div class="summary">
      <button class="stat" class:on={stat === 'week'} onclick={() => (stat = stat === 'week' ? null : 'week')} aria-expanded={stat === 'week'}>
        <strong>{todos.data ? week.length + weekTodos.length : todos.error ? '—' : '…'}</strong><span>7일 내 마감</span>
      </button>
      <button class="stat" class:on={stat === 'assign'} onclick={() => (stat = stat === 'assign' ? null : 'assign')} aria-expanded={stat === 'assign'}>
        <strong>{items.filter((i) => i.kind === 'assignment' && isPending(i)).length}</strong><span>남은 과제</span>
      </button>
      <button class="stat" class:on={stat === 'vod'} onclick={() => (stat = stat === 'vod' ? null : 'vod')} aria-expanded={stat === 'vod'}>
        <strong>{items.filter((i) => i.kind === 'vod' && isPending(i)).length}</strong><span>남은 강의</span>
      </button>
      <button class="stat" class:on={stat === 'todo'} onclick={() => (stat = stat === 'todo' ? null : 'todo')} aria-expanded={stat === 'todo'}>
        <strong>{todos.data ? remainingTodos.length : todos.error ? '—' : '…'}</strong><span>남은 할 일</span>
      </button>
      <button class="stat danger" class:on={stat === 'missed'} onclick={() => (stat = stat === 'missed' ? null : 'missed')} aria-expanded={stat === 'missed'}>
        <strong>{items.filter((i) => isOverdue(i)).length}</strong><span>놓친 항목</span>
      </button>
    </div>
    {#if stat}
      <section class="stat-list" data-kind={stat} class:card={statHasItems || statLoading}>
        {#if statLoading}
          <Skeleton rows={1} height={64} />
        {:else if statHasItems}
          <div class="list">
            {#each statTodos as t (t.id)}
              <TodoRow todo={t} showDate color={todoColor(t)} course={todoCourse(t)} onopen={editTodo} />
            {/each}
            {#each (stat === 'todo' ? [] : statItems) as item (item.key)}
              <AgendaItem
                {item}
                showDate
                color={colors.get(item.courseId) ?? 'var(--text-3)'}
                course={courseName(item.courseId)}
                onopen={(i) => (detailKey = i.key)}
                ontoggle={toggleDone}
              />
            {/each}
          </div>
        {:else if !statNeedsTodos || todos.data !== null}
          <EmptyState message={{ week: '7일 안에 마감할 일이 없어요', assign: '남은 과제가 없어요', vod: '남은 강의가 없어요', todo: '남은 할 일이 없어요', missed: '놓친 항목이 없어요' }[stat]} />
        {/if}
      </section>
    {/if}

    <div class="legend" aria-label="과목 필터">
      <button class="course all-courses" onclick={toggleAllCourses}>
        <Icon name={allCoursesSelected ? 'close' : 'tick'} size={14} />
        {allCoursesSelected ? '전체 해제' : '전체 선택'}
      </button>
      <button
        class="course"
        class:off={hidden.has(COMMON)}
        style:--c="var(--todo-neutral)"
        onclick={() => toggleCourse(COMMON)}
        aria-pressed={!hidden.has(COMMON)}
      >
        <span class="dot sq"></span>공통 할 일
      </button>
      {#each data.courses as c (c.id)}
        <button
          class="course"
          class:off={hidden.has(c.id)}
          style:--c={colors.get(c.id)}
          onclick={() => toggleCourse(c.id)}
          aria-pressed={!hidden.has(c.id)}
        >
          <span class="dot"></span>{c.name}
        </button>
      {/each}
    </div>

    <div class="board calendar-board" data-layout={side ? 'side' : 'wide'}>
      <div class="cal-wrap" bind:this={calWrap}>
        <MonthCalendar
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
        <div class="strip-anchor">
          <div class="strip" class:on={stripOn} inert={!stripOn} bind:clientHeight={stripH}>
            <MonthCalendar
              week
              bind:year
              bind:month
              bind:selected
              items={visible}
              {colors}
              {names}
              todos={myTodos}
              onpick={afterStripPick}
              onexpand={expand}
            />
          </div>
        </div>
      {/if}

      <div class="lists" style:--strip-h="{stripOn ? stripH : 0}px">
        <section class="agenda">
          <h2 class="day-title" bind:this={dayTitle}>
            {longDay(selected)}
            {#if selected === todayKey()}<span class="chip primary">오늘</span>{/if}
            <button class="add" onclick={() => addTodo()}><Icon name="plus" size={16} stroke={2.4} />할 일</button>
          </h2>
          {#if dayItems.length || dayTodos.length}
            <div class="list">
              {#each dayTodos as t (t.id)}
                <TodoRow todo={t} color={todoColor(t)} course={todoCourse(t)} onopen={editTodo} />
              {/each}
              {#each dayItems as item (item.key)}
                <AgendaItem
                  {item}
                  color={colors.get(item.courseId) ?? 'var(--text-3)'}
                  course={courseName(item.courseId)}
                  onopen={(i) => (detailKey = i.key)}
                  ontoggle={toggleDone}
                />
              {/each}
            </div>
          {:else}
            <EmptyState message="이날 마감인 일정이 없어요" />
          {/if}
        </section>

        <section class="upcoming">
          <h2 class="section-title up-title">다가오는 일정</h2>
          {#if upcoming.length || upcomingTodos.length}
            <div class="list">
              {#each upcomingTodos as t (t.id)}
                <TodoRow todo={t} showDate color={todoColor(t)} course={todoCourse(t)} onopen={editTodo} />
              {/each}
              {#each upcoming as item (item.key)}
                <AgendaItem
                  {item}
                  showDate
                  color={colors.get(item.courseId) ?? 'var(--text-3)'}
                  course={courseName(item.courseId)}
                  onopen={(i) => (detailKey = i.key)}
                  ontoggle={toggleDone}
                />
              {/each}
            </div>
          {:else}
            <EmptyState message="다가오는 일정이 없어요" />
          {/if}
        </section>
        {#if undated.length}
          <section class="undated">
            <h2 class="section-title">날짜 미정</h2>
            <div class="list">
              {#each undated as item (item.key)}
                <AgendaItem {item} color={colors.get(item.courseId) ?? 'var(--text-3)'} course={courseName(item.courseId)} onopen={(i) => (detailKey = i.key)} ontoggle={toggleDone} />
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
        {#if dayItems.length || dayTodos.length}
          <ul>
            {#each dayItems as item (item.key)}
              <li>
                <button
                  class="pop-row"
                  class:finished={isFinished(item)}
                  style:--c={colors.get(item.courseId) ?? 'var(--text-3)'}
                  onclick={() => openFromPop(item.key)}
                >
                  <i class="shape" class:vod={item.kind === 'vod'} aria-hidden="true"></i>
                  <span class="pt">{item.title}</span>
                  {#if item.due}<span class="ptime">{dueTime(item.due)}</span>{/if}
                </button>
              </li>
            {/each}
            {#each dayTodos as t (t.id)}
              <li>
                <button
                  class="pop-row"
                  class:finished={t.doneAt !== null}
                  style:--c={todoColor(t)}
                  onclick={() => {
                    dayPop = false;
                    editTodo(t);
                  }}
                >
                  <i class="shape todo" aria-hidden="true"></i>
                  <span class="pt">{t.title}</span>
                  <span class="ptime">{t.allDay || t.dueAt === null ? '하루 종일' : time(t.dueAt)}</span>
                </button>
              </li>
            {/each}
          </ul>
        {:else}
          <EmptyState message="이날 마감인 일정이 없어요" compact />
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

<TodoSheet bind:open={todoOpen} todo={editing} {draft} courses={data?.courses ?? []} {colors} />

<ItemSheet
  item={detail}
  subtodos={detail ? (todos.data ?? []).filter((t) => t.parentKey === detail.key) : []}
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

  .stat-list {
    margin-bottom: 12px;
    display: grid;
    gap: 8px;
  }

  .stat-list.card { padding: 12px; }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }

  .course {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 11px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 650;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    transition: opacity 0.15s;
  }

  .course .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--c);
  }

  .course .dot.sq {
    border-radius: 2px;
  }

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

  .course.off {
    opacity: 0.4;
    text-decoration: line-through;
  }

  .course.all-courses {
    color: var(--primary-text);
    background: var(--primary-weak);
    border-color: color-mix(in srgb, var(--primary) 25%, var(--border));
  }

  .board, .cal-wrap, .lists { min-width: 0; }

  .board[data-layout='wide'] { display: flex; flex-direction: column; width: 100%; }
  .board[data-layout='wide'] > .cal-wrap,
  .board[data-layout='wide'] > .lists { width: 100%; }

  .cal-wrap {
    scroll-margin-top: calc(var(--topbar-h, 64px) + 8px);
  }

  .strip-anchor {
    position: sticky;
    top: var(--topbar-h, 64px);
    z-index: 15;
    height: 0;
  }

  .strip {
    position: absolute;
    top: 0;
    left: -16px;
    right: -16px;
    opacity: 0;
    transform: translateY(-10px);
    pointer-events: none;
    transition:
      opacity 0.16s,
      transform 0.22s var(--ease);
  }

  .strip.on {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }

  @media (min-width: 640px) {
    .strip {
      left: -28px;
      right: -28px;
    }
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

    .legend {
      flex-wrap: nowrap;
      overflow-x: auto;
      scrollbar-width: none;
      margin: 0 -16px 12px;
      padding: 0 16px;
    }

    .legend::-webkit-scrollbar {
      display: none;
    }

    .course {
      flex: none;
    }
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
