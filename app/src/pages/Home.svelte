<script lang="ts">
  import { onMount } from 'svelte';
  import { isApp } from '../lib/api';
  import { toggleDone } from '../lib/actions.svelte';
  import { POLL_MS, classWatch, isAttended, lectureMark, markFor, markTitle, nextClass, sourceSuffix, useClassWatch } from '../lib/classwatch.svelte';
  import { currentMealIndex, isNowMeal, placePrice, sortedPlaces } from '../lib/meals';
  import { courseColors } from '../lib/colors';
  import { dayKey, dueDateTime, hourNow, todayKey } from '../lib/format';
  import { homeAgenda, type HomeEntry } from '../lib/home-agenda';
  import { settings } from '../lib/settings.svelte';
  import { calendar, lectures, meals, seatSession, seats, timetable, todos } from '../lib/store.svelte';
  import { go, openSeats } from '../lib/ui.svelte';
  import type { CalendarItem, Todo } from '../lib/types';
  import AgendaItem from '../components/AgendaItem.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import Icon from '../components/Icon.svelte';
  import ItemSheet from '../components/ItemSheet.svelte';
  import LoadError from '../components/LoadError.svelte';
  import MySeat from '../components/MySeat.svelte';
  import Ring from '../components/Ring.svelte';
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
    return useClassWatch();
  });

  const colors = $derived(courseColors(calendar.data?.courses ?? []));
  const courseName = (id: number) => calendar.data?.courses.find((c) => c.id === id)?.name ?? '';
  const agenda = $derived(homeAgenda(calendar.data?.items ?? [], todos.data ?? [], classWatch.now, settings.showUndatedAssignments));
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

  const active = $derived(lectures.at > classWatch.now - 10 * 60_000 ? (lectures.data?.items ?? []) : []);
  const openItems = $derived(active.filter((l) => !isAttended(lectureMark(l))));
  const done = $derived.by(() => {
    const cur = classWatch.current;
    const m = cur ? markFor(cur) : null;
    if (cur && isAttended(m)) return { name: cur.name, detail: `${cur.start} 수업${cur.room ? ` · ${cur.room}` : ''}`, mark: m };
    for (const l of active) {
      const lm = lectureMark(l);
      if (isAttended(lm)) return { name: l.name, detail: l.time, mark: lm };
    }
    return null;
  });
  const watching = $derived(classWatch.current && !done ? classWatch.current : null);
  const left = $derived(Math.max(0, Math.min(1, (classWatch.nextAt - classWatch.now) / POLL_MS)));
  const next = $derived(nextClass(classWatch.now));

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
  <h2 class="section-title">지금 출석</h2>
  <LoadError resource={lectures} what="출석 정보를" />
  {#if openItems.length}
    <button class="attend card live" onclick={() => go('attendance')}>
      <span class="pulse" aria-hidden="true"></span>
      <div>
        <span class="eyebrow">지금 출석할 수 있어요</span>
        <strong>{openItems[0].name}{openItems.length > 1 ? ` 외 ${openItems.length - 1}개` : ''}</strong>
        <span class="muted">{openItems[0].time}</span>
      </div>
      <span class="go">출석하기 <Icon name="right" size={18} /></span>
    </button>
  {:else if done}
    <button class="attend card done" onclick={() => go('attendance')}>
      <span class="badge" aria-hidden="true"><Icon name="tick" size={18} stroke={2.6} /></span>
      <div>
        <strong>{done.name} · {markTitle(done.mark)}</strong>
        <span class="muted">{done.detail}{sourceSuffix(done.mark)}</span>
      </div>
      <Icon name="right" size={18} />
    </button>
  {:else if watching}
    <button class="attend card watch" onclick={() => go('attendance')}>
      <Ring value={left} size={42} stroke={3.5}>
        <span class="watch-icon" class:spin={classWatch.polling}><Icon name="refresh" size={18} stroke={2.2} /></span>
      </Ring>
      <div>
        <span class="eyebrow">{watching.start} 수업 · 출석 열리는지 확인 중</span>
        <strong>{watching.name}</strong>
        <span class="muted" aria-live="polite">{classWatch.polling ? '확인하는 중…' : '5초마다 자동으로 확인해요'}{watching.room ? ` · ${watching.room}` : ''}</span>
      </div>
      <Icon name="right" size={18} />
    </button>
  {:else}
    <button class="attend card idle" onclick={() => go('attendance')}>
      <Icon name="check" size={28} />
      <div>
        <strong>{lectures.error ? '출석 정보를 확인해 주세요' : !lectures.data ? '출석 가능한 수업을 확인하고 있어요' : '지금 출석 가능한 수업이 없어요'}</strong>
        <span class="muted">{next ? `다음 수업 ${next.start} · ${next.name}` : '출결 현황과 시간표를 확인해 보세요'}</span>
      </div>
      <Icon name="right" size={18} />
    </button>
  {/if}
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
            <EmptyState message={group.id === 'today' ? '오늘 마감할 일은 없어요' : '다가오는 일정이 없어요'} />
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
    <div class="empty card"><strong>오늘은 학식 메뉴가 없어요</strong>주말·공휴일에는 운영하지 않을 수 있어요</div>
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
          <ul class="menu">
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
        <p class="late-hint">{entry.value.lateUntil ? `늦은 제출 기한 ${dueDateTime(entry.value.lateUntil)} · 상세에서 확인` : '늦은 제출 가능 여부를 상세에서 확인해 주세요'}</p>
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
  subtodos={detail ? (todos.data ?? []).filter((t) => t.parentKey === detail.key) : []}
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
  .idle { border-color: color-mix(in srgb, var(--primary) 35%, var(--border)); }
  .idle > :global(svg) { color: var(--primary-text); }

  .attend {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 18px;
    text-align: left;
    color: var(--text-2);
  }

  .attend div {
    flex: 1;
    display: grid;
    gap: 1px;
  }

  .attend strong {
    font-size: 16px;
    font-weight: 700;
    color: var(--text);
  }

  .attend .muted {
    font-size: 13px;
  }

  .live {
    background: var(--ok-weak);
    border: 0;
    color: #fff;
    box-shadow: none;
  }

  .live strong,
  .live .muted,
  .live .eyebrow {
    color: var(--ok);
  }

  .live .eyebrow {
    font-size: 12.5px;
    font-weight: 700;
    opacity: 0.9;
  }

  .go {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    font-weight: 750;
    white-space: nowrap;
  }

  .pulse {
    width: 12px;
    height: 12px;
    border-radius: 999px;
    background: var(--ok);
  }

  @keyframes ping {
    70% {
      box-shadow: 0 0 0 12px rgb(255 255 255 / 0%);
    }
  }

  .done {
    background: var(--ok-weak);
    border-color: color-mix(in srgb, var(--ok) 30%, transparent);
  }

  .done strong {
    font-size: 16px;
  }


  .badge {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 999px;
    background: var(--ok);
    color: var(--surface);
    flex: none;
  }

  .watch {
    border-color: var(--primary);
    box-shadow: none;
  }

  .watch .eyebrow {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--primary-text);
  }

  .watch-icon {
    display: grid;
    color: var(--primary-text);
  }

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

  .meal ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 3px;
  }

  .meal li {
    font-size: 14.5px;
    font-weight: 650;
    color: var(--text);
  }

  .meal ul.menu {
    gap: 4px;
  }

  .meal .menu li {
    font-size: 14px;
    font-weight: 500;
    line-height: 1.45;
    letter-spacing: -0.01em;
  }

  .meals.swipe ul.menu {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    grid-template-rows: repeat(4, auto);
    grid-auto-flow: column;
    grid-auto-columns: minmax(0, 1fr);
    gap: 5px 10px;
  }

  .meals.swipe .menu li {
    font-size: 13px;
    line-height: 1.55;
    letter-spacing: -0.02em;
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
  .attend { min-height: 88px; padding: 18px 16px; gap: 12px; }
  .attend div { min-width: 0; gap: 5px; }
  .attend > :global(svg:last-child) { flex: none; }
  .idle { border-color: var(--border); }
  .live { color: var(--ok); border: 1px solid color-mix(in srgb, var(--ok) 25%, var(--border)); }
  .agenda-label { font-weight: 600; }
  .agenda-label span { font-size: 12px; padding: 0 5px; background: var(--surface-3); border-radius: 5px; }
  .show-more { min-height: 44px; }
  .a-context { min-width: 0; }
  .meal-title { flex-wrap: wrap; gap: 5px 8px; }
  .meal header { padding-bottom: 10px; border-bottom: 1px solid var(--border); }
  .meal .menu li:first-child { font-weight: 700; }

</style>
