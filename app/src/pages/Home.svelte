<script lang="ts">
  import { onMount } from 'svelte';
  import { isApp } from '../lib/api';
  import { toggleDone } from '../lib/actions.svelte';
  import { classWatch } from '../lib/classwatch.svelte';
  import { currentMealIndex, isNowMeal, placePrice, sortedPlaces } from '../lib/meals';
  import { courseColors } from '../lib/colors';
  import { dayKey, dueDateTime, hourNow, todayKey } from '../lib/format';
  import { homeAgenda, type HomeEntry } from '../lib/home-agenda';
  import { settings } from '../lib/settings.svelte';
  import { displayedTodos } from '../lib/todos.svelte';
  import { calendar, meals, seatSession, seats, timetable, todos } from '../lib/store.svelte';
  import { go, openSeats } from '../lib/ui.svelte';
  import type { CalendarItem, Todo } from '../lib/types';
  import AgendaItem from '../components/AgendaItem.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';
  import ItemSheet from '../components/ItemSheet.svelte';
  import LoadError from '../components/LoadError.svelte';
  import MySeat from '../components/MySeat.svelte';
  import CurrentAttendance from '../components/CurrentAttendance.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import TodoRow from '../components/TodoRow.svelte';
  import TodoSheet from '../components/TodoSheet.svelte';

  let detailKey = $state<string | null>(null);
  const detail = $derived(calendar.data?.items.find((i) => i.key === detailKey) ?? null);

  onMount(() => {
    seatSession.load();
    seats.load();
    calendar.load();
    meals.load();
    timetable.load();
    todos.load();
  });

  const colors = $derived(courseColors(calendar.data?.courses ?? []));
  const courseName = (id: number) => calendar.data?.courses.find((c) => c.id === id)?.name ?? '';
  const agenda = $derived(homeAgenda(calendar.data?.items ?? [], displayedTodos(), classWatch.now, settings.showUndatedAssignments));
  const agendaLoading = $derived((calendar.data === null && !calendar.error) || (todos.data === null && !todos.error));
  const agendaComplete = $derived(calendar.data !== null && todos.data !== null);
  const groups = $derived([
    { id: 'today', title: '오늘 마감', entries: agenda.today, limit: 6 },
    { id: 'overdue', title: '기한 지남 · 확인 필요', entries: agenda.overdue, limit: 4 },
    { id: 'upcoming', title: '다가오는 일정', entries: agenda.upcoming, limit: 4 },
    { id: 'undated', title: '날짜 미정', entries: agenda.undated, limit: 3 },
  ]);
  let expanded = $state<string[]>([]);
  let todoOpen = $state(false);
  let editing = $state<Todo | null>(null);
  let draft = $state<{ date?: string; courseId?: number; parentKey?: string; parentTitle?: string }>({});
  function addTodo(item?: CalendarItem) {
    editing = null;
    draft = { date: dayKey(classWatch.now), ...(item ? { courseId: item.courseId, parentKey: item.key, parentTitle: item.title } : {}) };
    todoOpen = true;
  }
  function editTodo(todo: Todo) {
    editing = todo;
    todoOpen = true;
  }
  const session = $derived(seatSession.data?.session ?? null);

  const todayPlace = $derived.by(() => {
    const day = meals.data?.find((d) => d.date === todayKey());
    return day ? (sortedPlaces(day)[0] ?? null) : null;
  });
  const nowIndex = $derived(todayPlace ? currentMealIndex(todayPlace.meals.map((m) => m.name), hourNow()) : 0);
  let strip: HTMLDivElement | undefined = $state();
  let shown = $state(0);

  $effect(() => {
    if (!isApp || !strip || !todayPlace) return;
    const card = strip.children[nowIndex] as HTMLElement | undefined;
    strip.scrollTo({ left: card ? card.offsetLeft - strip.offsetLeft : 0 });
    shown = nowIndex;
  });

  function onStripScroll() {
    if (!strip) return;
    const w = strip.clientWidth;
    shown = Math.round(strip.scrollLeft / Math.max(1, w * 0.86));
  }

</script>

<div class="page home">
  <section class="a-attend" aria-label="출석">
  <h2 class="section-title">빠른 출결 <button class="link" onclick={() => go('attendance')}>출결</button></h2>
  <CurrentAttendance showLabel={false} />
  </section>

  <section class="a-due" aria-label="오늘 할 일">
    <h2 class="section-title">오늘 할 일 <button class="link" onclick={() => go('calendar')}>캘린더</button></h2>
    <LoadError resource={calendar} what="과제·강의를" />
    <LoadError resource={todos} what="내 할 일을" />
    {#each groups as group (group.id)}
      {#if agendaLoading ? group.id === 'today' || group.id === 'upcoming' : group.entries.length || ((group.id === 'today' || group.id === 'upcoming') && agendaComplete)}
        <div class="agenda-group" data-group={group.id} aria-busy={agendaLoading}>
          <h3 class="agenda-label" class:urgent={group.id === 'overdue'}>{group.title}{#if !agendaLoading}<span>{group.entries.length}</span>{/if}</h3>
          {#if agendaLoading}
            <Skeleton rows={group.id === 'today' ? 1 : 2} height={82} />
          {:else if !group.entries.length}
            <EmptyState message={group.id === 'today' ? '오늘 마감할 일은 없어요.' : '다가오는 일정이 없어요.'} />
          {:else}
            <div class="list">
              {#each (expanded.includes(group.id) ? group.entries : group.entries.slice(0, group.limit)) as entry (entry.key)}
                {@render agendaRow(entry)}
              {/each}
            </div>
            {#if group.entries.length > group.limit}
              <button class="show-more" onclick={() => expanded = expanded.includes(group.id) ? expanded.filter((id) => id !== group.id) : [...expanded, group.id]}>
                {expanded.includes(group.id) ? '접기' : `${group.entries.length - group.limit}개 더 보기`}
              </button>
            {/if}
          {/if}
        </div>
      {/if}
    {/each}
  </section>

  <div class="a-context">
  <section class="a-seat">
  <h2 class="section-title">열람실 <button class="link" onclick={() => go('seats')}>좌석 지도</button></h2>
  <LoadError resource={seatSession} what="내 좌석을" />
  <LoadError resource={seats} what="열람실 좌석을" />
  {#if session}
    <MySeat {session} compact />
  {:else if seats.data}
    <details class="seat-fold card" open>
    <summary><span>빈자리 현황</span><span class="muted">{seats.data.buildings.reduce((n, b) => n + b.rooms.reduce((sum, r) => sum + r.free, 0), 0)}석 <Icon name="down" size={15} /></span></summary>
    <div class="seat-summary">
      {#each seats.data.buildings as b (b.id)}
        {@const free = b.rooms.reduce((n, r) => n + r.free, 0)}
        {@const total = b.rooms.reduce((n, r) => n + r.total, 0)}
        <button class="bld" onclick={() => openSeats(b.id)} aria-label="{b.name}({b.id}동) 좌석 보기">
          <span class="building-name">{b.name}<small class="dong">({b.id}동)</small></span>
          <strong>{free}</strong>
          <span class="bar"><i style:width="{total ? 100 - Math.round((free / total) * 100) : 0}%"></i></span>
        </button>
      {/each}
    </div>
    </details>
  {:else if !seats.error}
    <Skeleton rows={1} height={96} />
  {/if}
  </section>

  </div>
  <section class="a-meal">
  <h2 class="section-title">
    <span class="meal-title">
      오늘 학식{todayPlace ? ` · ${todayPlace.name}` : ''}
      {#if todayPlace && placePrice(todayPlace)}<span class="chip primary">{placePrice(todayPlace)}</span>{/if}
    </span>
    <button class="link" onclick={() => go('meals')}>식단 전체</button>
  </h2>
  {#if !meals.data}
    {#if meals.error}<LoadError resource={meals} what="학식을" stale={false} />{:else}<Skeleton rows={1} height={150} />{/if}
  {:else if !todayPlace}
    <div class="empty card"><strong>오늘은 학식 메뉴가 없어요.</strong>주말·공휴일에는 운영하지 않을 수 있어요.</div>
  {:else}
    <div class="meals" class:swipe={isApp} bind:this={strip} onscroll={onStripScroll}>
      {#each todayPlace.meals as meal (meal.name)}
        {@const now = isNowMeal(meal.name, hourNow())}
        <article class="meal card" class:now>
          <header>
            <strong>{meal.name}</strong>
            {#if now}<span class="chip primary">지금</span>{/if}
            <span class="time">{meal.start}~{meal.end}</span>
          </header>
          <ul class="menu" style:--menu-rows={Math.max(7, Math.ceil(meal.items.length / 2))}>
            {#each meal.items as food, j (j)}
              <li>{food}</li>
            {/each}
          </ul>
        </article>
      {/each}
    </div>
    {#if isApp && todayPlace.meals.length > 1}
      <div class="dots" aria-hidden="true">
        {#each todayPlace.meals as _, i (i)}<i class:on={i === shown}></i>{/each}
      </div>
    {/if}
  {/if}
  </section>
</div>

{#snippet agendaRow(entry: HomeEntry)}
  {#if entry.kind === 'todo'}
    <TodoRow todo={entry.value} showDate color={entry.value.courseId === null ? 'var(--todo-neutral)' : colors.get(entry.value.courseId) ?? 'var(--text-3)'} course={entry.value.courseId === null ? '공통' : courseName(entry.value.courseId)} onopen={editTodo} />
  {:else}
    <div>
      <AgendaItem item={entry.value} showDate color={colors.get(entry.value.courseId) ?? 'var(--text-3)'} course={courseName(entry.value.courseId)} onopen={(i) => detailKey = i.key} ontoggle={toggleDone} />
      {#if entry.value.kind === 'assignment' && entry.due !== null && entry.due * 1000 <= classWatch.now}
        <p class="late-hint">{entry.value.lateUntil ? `지각 제출 마감 ${dueDateTime(entry.value.lateUntil)} · 상세에서 확인` : '지각 제출 가능 여부를 상세에서 확인해 주세요.'}</p>
      {/if}
    </div>
  {/if}
{/snippet}

<ItemSheet
  item={detail}
  onclose={() => (detailKey = null)}
  course={detail ? courseName(detail.courseId) : ''}
  color={detail ? (colors.get(detail.courseId) ?? 'var(--text-3)') : ''}
  ontoggle={toggleDone}
  subtodos={detail ? displayedTodos().filter((t) => t.parentKey === detail.key) : []}
  onaddtodo={addTodo}
  oneditodo={editTodo}
/>
<TodoSheet bind:open={todoOpen} todo={editing} {draft} courses={calendar.data?.courses ?? []} {colors} />

<style>
  .agenda-group { margin-bottom: 18px; }
  .agenda-label { display: flex; align-items: center; gap: 7px; font-size: 13px; color: var(--text-2); margin: 0 3px 8px; }
  .agenda-label span { color: var(--text-3); font-variant-numeric: tabular-nums; }
  .agenda-label.urgent { color: var(--warn); }
  .show-more { width: 100%; padding: 10px; color: var(--primary-text); font-size: 13px; font-weight: 650; }
  .late-hint { margin: 5px 6px 2px; color: var(--text-3); font-size: 12px; }
  .seat-fold { overflow: hidden; }
  .seat-fold summary { display: flex; justify-content: space-between; align-items: center; padding: 16px; cursor: pointer; font-size: 14px; font-weight: 650; list-style: none; }
  .seat-fold summary::-webkit-details-marker { display: none; }
  .seat-fold summary span { display: flex; align-items: center; gap: 7px; }
  .seat-fold .seat-summary { border-top: 1px solid var(--border); }
  .a-attend { margin-bottom: 0; }
  .link {
    color: var(--primary-text);
    font-size: 13px;
    font-weight: 650;
  }

  .seat-summary {
    width: 100%;
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px;
    padding: 14px;
    text-align: left;
  }

  .bld {
    text-align: left;
    border-radius: 10px;
    display: grid;
    gap: 4px;
    padding: 4px 6px;
  }

  .bld:hover { background: var(--surface-2); }

  .building-name {
    display: inline-flex;
    align-items: baseline;
    gap: 2px;
    white-space: nowrap;
    font-size: 12.5px;
    font-weight: 650;
    color: var(--text-2);
  }

  .dong { font-size: 0.78em; font-weight: 550; color: var(--text-3); }

  .bld strong {
    font-size: 24px;
    font-weight: 800;
    color: var(--seat-free);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.03em;
  }

  .bld strong::after {
    content: ' 빈 자리';
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    letter-spacing: 0;
  }

  .bar {
    height: 5px;
    border-radius: 999px;
    background: var(--surface-3);
    overflow: hidden;
  }

  .bar i {
    display: block;
    height: 100%;
    background: var(--primary);
    border-radius: 999px;
  }

  .list {
    display: grid;
    gap: 8px;
  }

  .meals {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 10px;
  }

  .meals.swipe {
    display: flex;
    overflow-x: auto;
    scroll-snap-type: x mandatory;
    scrollbar-width: none;
    margin: 0 -16px;
    padding: 2px 16px 4px;
  }

  .meals.swipe::-webkit-scrollbar {
    display: none;
  }

  .meals.swipe .meal {
    flex: 0 0 86%;
    scroll-snap-align: center;
  }

  .meal {
    display: grid;
    align-content: start;
    gap: 10px;
    padding: 16px;
  }

  .meal.now {
    border-color: var(--primary);
    box-shadow: none;
  }

  .meal header {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .meal header strong {
    font-size: 16px;
    font-weight: 750;
  }

  .meal .time {
    margin-left: auto;
    font-size: 12.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .meal .menu {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  .meal .menu li {
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    line-height: 1.45;
    letter-spacing: -0.01em;
  }

  @media (max-width: 767px) {
    .meals:not(.swipe) {
      grid-template-columns: minmax(0, 1fr);
    }

    .meal .menu {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      grid-template-rows: repeat(var(--menu-rows), minmax(1.55em, auto));
      grid-auto-flow: column;
      align-content: start;
      align-items: start;
      gap: 5px 12px;
      font-size: 13px;
    }

    .meal .menu li {
      font-size: inherit;
      line-height: 1.55;
      letter-spacing: -0.02em;
    }
  }

  .home {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }

  @media (min-width: 1024px) {
    .home {
      grid-template-columns: minmax(0, 1.55fr) minmax(0, 1fr);
      grid-template-areas:
        'attend attend'
        'due context'
        'meal meal';
      column-gap: 24px;
      align-items: start;
    }

    .a-attend {
      grid-area: attend;
    }

    .a-due {
      grid-area: due;
    }

    .a-context {
      grid-area: context;
    }

    .a-meal {
      grid-area: meal;
    }

    .seat-summary {
      grid-template-columns: 1fr;
      gap: 10px;
    }

    .bld {
      grid-template-columns: minmax(0, 1fr) auto;
      align-items: center;
    }

    .bld .bar {
      grid-column: 1 / -1;
    }
  }

  .meal-title {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .dots {
    display: flex;
    justify-content: center;
    gap: 6px;
    margin-top: 10px;
  }

  .dots i {
    width: 6px;
    height: 6px;
    border-radius: 999px;
    background: var(--border-strong);
    transition: all 0.2s;
  }

  .dots i.on {
    width: 18px;
    background: var(--primary);
  }

  .a-attend .section-title { margin-top: 22px; }
  .agenda-label { font-weight: 600; }
  .agenda-label span { font-size: 12px; padding: 0 5px; background: var(--surface-3); border-radius: 5px; }
  .show-more { min-height: 44px; }
  .a-context { min-width: 0; }
  .meal-title { flex-wrap: wrap; gap: 5px 8px; }
  .meal header { padding-bottom: 10px; border-bottom: 1px solid var(--border); }

</style>
