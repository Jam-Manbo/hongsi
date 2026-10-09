<script lang="ts">
  import { onMount } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { isApp } from '../shared/api/api';
  import {
    classWatch,
    classSessions,
    inWindow,
    periodLabel,
    sessionState,
    todayClasses,
  } from '../features/attendance/classwatch.svelte';
  import { ago, WEEKDAYS } from '../shared/utils/format';
  import { courseColors } from '../features/calendar/colors';
  import { attendance, timetable } from '../features/attendance/attendance-resources.svelte';
  import { calendar } from '../features/calendar/calendar-resources.svelte';
  import { pref, setPref } from '../shared/state/preferences';
  import { focus, toast } from '../shared/state/ui.svelte';
  import type { AttendanceMark, AttendanceWeek, MarkKind } from '../shared/types';
  import Icon from '../shared/ui/Icon.svelte';
  import LoadError from '../shared/ui/LoadError.svelte';
  import Popover from '../shared/ui/Popover.svelte';
  import CurrentAttendance from '../features/attendance/CurrentAttendance.svelte';
  import Sheet from '../shared/ui/Sheet.svelte';
  import Skeleton from '../shared/ui/Skeleton.svelte';
  import EmptyState from '../shared/ui/EmptyState.svelte';
  import WeekTimetable from '../features/attendance/WeekTimetable.svelte';

  const phone = new MediaQuery('max-width: 639px');
  const desktop = new MediaQuery('min-width: 1024px');
  let view = $state<'today' | 'week'>(pref('attendance-view', 'today'));
  let weekOpen = $state(false);
  const showToday = $derived(desktop.current || phone.current || view === 'today');
  const weekInline = $derived(desktop.current || (!phone.current && view === 'week'));

  async function refreshTimetable() {
    await timetable.refresh();
    if (timetable.error) toast('시간표를 새로고침하지 못했어요. 저장된 정보는 유지됩니다.', 'error');
    else if (timetable.data) toast('시간표를 새로고침했어요.', 'success');
  }

  function setView(v: 'today' | 'week') {
    view = v;
    setPref('attendance-view', v);
  }

  onMount(() => {
    attendance.load();
    calendar.load();
    timetable.load();
  });

  $effect(() => {
    if (focus.timetable) {
      focus.timetable = false;
      weekOpen = true;
    }
  });

  const today = $derived(todayClasses(timetable.data?.slots ?? [], classWatch.now));


  const colorByCode = $derived.by(() => {
    const term = calendar.data?.currentTerm;
    const courses = (calendar.data?.courses ?? []).filter((course) => !term
      || (course.term?.year === term.year && course.term?.semester === term.semester));
    const colors = courseColors(courses);
    return new Map(courses.map((c) => [c.code ?? '', colors.get(c.id) ?? 'var(--border-strong)']));
  });

  const MARK: Record<MarkKind, { label: string; cls: string }> = {
    present: { label: '출석', cls: 'present' },
    late: { label: '지각', cls: 'late' },
    absent: { label: '결석', cls: 'absent' },
    excused: { label: '공결', cls: 'excused' },
    none: { label: '기록 없음', cls: 'none' },
    planned: { label: '예정', cls: 'planned' },
    other: { label: '기타', cls: 'none' },
  };

  const KST = 9 * 3600_000;

  function withWeekday(md: string): string {
    const m = /^(\d{1,2})\/(\d{1,2})$/.exec(md);
    if (!m) return '날짜 미입력';
    const k = new Date(Date.now() + KST);
    const [mon, day] = [Number(m[1]), Number(m[2])];
    let y = k.getUTCFullYear();
    if (mon - (k.getUTCMonth() + 1) > 6) y -= 1;
    else if (k.getUTCMonth() + 1 - mon > 6) y += 1;
    return `${mon}/${day}(${WEEKDAYS[new Date(Date.UTC(y, mon - 1, day)).getUTCDay()]})`;
  }

  const markLabel = (s: AttendanceMark) => (s.kind === 'other' ? s.mark || MARK.other.label : MARK[s.kind].label);

  function sessionRows(w: AttendanceWeek) {
    const rows: { date: string; label: string; cls: string; n: number }[] = [];
    for (const s of w.sessions) {
      const [date, label] = [withWeekday(s.date), markLabel(s)];
      const last = rows[rows.length - 1];
      if (last && last.date === date && last.label === label) last.n += 1;
      else rows.push({ date, label, cls: MARK[s.kind].cls, n: 1 });
    }
    return rows;
  }

  const weekLabel = (w: AttendanceWeek) =>
    `${w.week}주차: ${sessionRows(w).map((r) => `${r.date} ${r.label}${r.n > 1 ? ` ${r.n}회` : ''}`).join(', ') || '기록 없음'}`;

  const fine = new MediaQuery('(hover: hover) and (pointer: fine)');
  let tip = $state<{ el: HTMLElement; id: string; week: AttendanceWeek } | null>(null);
  let tipOpen = $state(false);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  function showTip(el: HTMLElement, id: string, week: AttendanceWeek) {
    clearTimeout(leaveTimer);
    tip = { el, id, week };
    tipOpen = true;
  }

  function toggleTip(e: MouseEvent, id: string, week: AttendanceWeek) {
    if (tipOpen && tip?.id === id && !fine.current) tipOpen = false;
    else showTip(e.currentTarget as HTMLElement, id, week);
  }

  function hoverTip(e: MouseEvent, id: string, week: AttendanceWeek) {
    if (fine.current) showTip(e.currentTarget as HTMLElement, id, week);
  }

  function leaveTip() {
    if (!fine.current) return;
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => (tipOpen = false), 250);
  }

</script>

<div class="page">
  <div class="top-row">
  <div class="left-col">
  <CurrentAttendance />

  {#if showToday}
  <section class="today-wrap">
  <h2 class="section-title">
    <span class="title-text">
      오늘 수업
    </span>
    {#if phone.current}
      <button class="link week-link" onclick={() => (weekOpen = true)}><Icon name="table" size={16} />주간 시간표</button>
    {:else if !desktop.current}
      {@render viewSwitch()}
    {/if}
  </h2>
  <LoadError resource={timetable} what="시간표를" stale={false} />
  {#if !timetable.data}
    {#if !timetable.error}<Skeleton rows={2} height={52} />{/if}
  {:else if today.length}
    <ul class="today card">
      {#each today as c (c.name + c.at)}
        {@const sessions = classSessions(c)}
        <li style:--c={colorByCode.get(c.code ?? '') ?? 'var(--border-strong)'} class:live={sessions.some((session) => inWindow(session, classWatch.now))}>
          <span class="t">{c.start}</span>
          <div class="info">
            <strong>{c.name}</strong>
            <span class="muted">{periodLabel(c)}{c.room ? ` · ${c.room}` : ''}</span>
          </div>
          <div class="session-tags" aria-label="교시별 출석 상태">
            {#each sessions as session (session.at)}
              {@const state = sessionState(session)}
              {#if state}<span class="chip {state.cls}" title="{session.start} · {periodLabel(session)}">{sessions.length > 1 ? `${session.round}회차 ` : ''}{state.label}</span>{/if}
            {/each}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState message="오늘은 수업이 없어요." />
  {/if}
  </section>
  {/if}
  </div>

  {#if weekInline}
    <section class="week-wrap">
      <h2 class="section-title">
        <span class="title-text">주간 시간표</span>
        <span class="week-actions">
          {#if !desktop.current}{@render viewSwitch()}{/if}
          {@render timetableRefresh()}
        </span>
      </h2>
      <LoadError resource={timetable} what="시간표를" />
      {#if timetable.data}
        <div class="card tt-card">
          <WeekTimetable slots={timetable.data.slots} colors={colorByCode} now={classWatch.now} />
        </div>
      {:else if !timetable.error}
        <Skeleton rows={1} height={420} />
      {/if}
    </section>
  {/if}
  </div>

  {#snippet viewSwitch()}
    <span class="seg" role="group" aria-label="시간표 보기">
      <button aria-pressed={view === 'today'} onclick={() => setView('today')}>오늘</button>
      <button aria-pressed={view === 'week'} onclick={() => setView('week')}>주간</button>
    </span>
  {/snippet}

  <div class="status-head" class:stack={isApp}>
    <h2>과목별 출결 현황</h2>
    <div class="legend" aria-label="출결 표시 색">
      {#each ['present', 'late', 'absent', 'none', 'planned'] as k (k)}
        <span><i class="mark {MARK[k as MarkKind].cls}"></i>{MARK[k as MarkKind].label}</span>
      {/each}
    </div>
    {#if attendance.at}<span class="updated muted small">{ago(attendance.at / 1000)} 업데이트</span>{/if}
  </div>
  <LoadError resource={attendance} what="출결 현황을" stale={false} hideParseError />
  {#if !attendance.data}
    {#if !attendance.error}<Skeleton rows={3} height={120} />{/if}
  {:else}
    <div class="courses">
      {#each attendance.data as c (c.code)}
        <article class="course card" style:--c={colorByCode.get(c.code) ?? 'var(--border-strong)'}>
          <header>
            <div>
              <h3>{c.name}</h3>
              <span class="course-meta muted small">{c.code}{#if c.cyber}<span class="cyber-label">사이버</span>{/if}</span>
            </div>
            {#if c.published}
              <div class="counts">
                <span class="chip ok">출석 {c.summary.present}</span>
                {#if c.summary.late}<span class="chip warn">지각 {c.summary.late}</span>{/if}
                {#if c.summary.absent}<span class="chip danger">결석 {c.summary.absent}</span>{/if}
                {#if c.summary.none}<span class="chip">기록 없음 {c.summary.none}</span>{/if}
              </div>
            {/if}
          </header>
          {#if c.published}
            <div class="weeks" role="group" aria-label="{c.name} 주차별 출결 (칸을 누르면 날짜가 보여요)">
              {#each c.weeks as w (w.week)}
                {@const id = `${c.code}:${w.week}`}
                <button
                  class="week"
                  class:on={tipOpen && tip?.id === id}
                  aria-label={weekLabel(w)}
                  aria-expanded={tipOpen && tip?.id === id}
                  onclick={(e) => toggleTip(e, id, w)}
                  onmouseenter={(e) => hoverTip(e, id, w)}
                  onmouseleave={leaveTip}
                >
                  <span class="wk">{w.week}</span>
                  <span class="marks" aria-hidden="true">
                    {#each w.sessions as s, i (i)}
                      <i class="mark {MARK[s.kind].cls}"></i>
                    {:else}
                      <span class="empty-week">—</span>
                    {/each}
                  </span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="notice">{c.notice ?? '교수님이 출석부를 공개하지 않았어요.'}</p>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</div>

<Popover anchor={tip?.el ?? null} bind:open={tipOpen} placement="top" role="tooltip" timeout={fine.current ? 0 : 4000}>
  {#if tip}
    <div class="tip">
      <strong>{tip.week.week}주차</strong>
      {#each sessionRows(tip.week) as r, i (i)}
        <span class="tip-row"><i class="mark {r.cls}"></i>{r.date} · {r.label}{#if r.n > 1}<b class="times">×{r.n}</b>{/if}</span>
      {:else}
        <span class="tip-row">기록 없음</span>
      {/each}
    </div>
  {/if}
</Popover>

{#snippet timetableRefresh()}
  <button class="icon-btn" aria-label="시간표 새로고침" title="시간표 새로고침" disabled={timetable.loading} onclick={refreshTimetable}>
    <span class:spin={timetable.loading}><Icon name="refresh" size={18} /></span>
  </button>
{/snippet}

<Sheet bind:open={weekOpen} title="주간 시간표" headerActions={timetableRefresh} wide>
  <LoadError resource={timetable} what="시간표를" />
  {#if timetable.data}
    <WeekTimetable slots={timetable.data.slots} colors={colorByCode} now={classWatch.now} compact />
  {:else if !timetable.error}
    <Skeleton rows={4} height={60} />
  {/if}
</Sheet>


<style>
  .course-meta { display: inline-flex; align-items: center; gap: 8px; }
  .cyber-label { padding: 1px 6px; border-radius: 5px; background: var(--surface-2); font-size: 11px; }
  .empty-week { color: var(--muted); font-size: 11px; line-height: 12px; }

  .week-actions { display: flex; align-items: center; gap: 8px; }

  .today {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }

  .today li {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 15px 16px;
  }

  .today li + li {
    border-top: 1px solid var(--border);
  }

  .today li::before {
    content: '';
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 0 4px 4px 0;
    background: var(--c);
  }

  .today li.live .t {
    color: var(--primary-text);
  }

  .t {
    width: 44px;
    font-size: 15px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }

  .session-tags {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 5px;
    max-width: 44%;
    flex-shrink: 0;
  }

  .session-tags .chip { white-space: nowrap; }

  .info {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  .info strong {
    font-size: 14.5px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: normal;
  }

  .info .muted {
    font-size: 12.5px;
  }

  @media (min-width: 1024px) {
    .top-row {
      display: grid;
      grid-template-columns: minmax(0, 0.9fr) minmax(0, 1.1fr);
      gap: 24px;
      align-items: start;
    }

    .courses {
      grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    }
  }

  .small { font-size: 12.5px; font-weight: 500; }

  .courses {
    display: grid;
    gap: 10px;
    grid-template-columns: repeat(auto-fill, minmax(min(320px, 100%), 1fr));
  }

  .course {
    position: relative;
    padding: 18px 18px 18px 20px;
    display: grid;
    align-content: start;
    gap: 12px;
    overflow: hidden;
  }

  .course::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: var(--c);
  }

  header {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }

  h3 {
    font-size: 16px;
    font-weight: 750;
  }

  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: flex-start;
  }

  .weeks {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(38px, 1fr));
    gap: 6px;
  }

  .week {
    display: grid;
    justify-items: center;
    align-content: start;
    gap: 4px;
    padding: 6px 2px;
    border-radius: 10px;
    background: var(--surface-2);
    transition:
      background 0.15s,
      box-shadow 0.15s;
  }

  .week:hover,
  .week.on {
    background: var(--surface-3);
  }

  .week.on {
    box-shadow: inset 0 0 0 1.5px var(--border-strong);
  }

  .tip {
    display: grid;
    gap: 3px;
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }

  .tip strong {
    font-size: 12px;
    font-weight: 750;
    opacity: 0.75;
  }

  .tip-row {
    display: flex;
    align-items: center;
    gap: 7px;
    font-variant-numeric: tabular-nums;
  }

  .times {
    margin-left: -2px;
    font-weight: 700;
    opacity: 0.7;
  }

  .title-text {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .week-link {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--primary-text);
    font-size: 13px;
    font-weight: 650;
  }

  .tt-card {
    padding: 12px 12px 14px 6px;
  }

  .wk {
    font-size: 11px;
    font-weight: 700;
    color: var(--text-3);
  }

  .marks {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 3px;
  }

  .mark {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 3px;
  }

  .mark.present {
    background: var(--ok);
  }
  .mark.late {
    background: var(--warn);
  }
  .mark.absent {
    background: var(--danger);
  }
  .mark.excused {
    background: var(--info);
  }
  .mark.none {
    background: var(--text-3);
    opacity: 0.55;
  }
  .mark.planned {
    background: transparent;
    box-shadow: inset 0 0 0 1.5px var(--border-strong);
  }

  .notice {
    font-size: 13.5px;
    color: var(--text-3);
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface-2);
  }

  .status-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    margin: 26px 4px 10px;
  }

  .status-head h2 {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-2);
    letter-spacing: -0.01em;
  }

  .status-head .updated {
    margin-left: auto;
  }

  .status-head.stack {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .status-head.stack .legend {
    grid-column: 1 / -1;
    order: 3;
  }

  @media (max-width: 639px) {
    .status-head {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .status-head .legend {
      grid-column: 1 / -1;
      order: 3;
    }
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 12px;
    font-weight: 550;
    color: var(--text-2);
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    white-space: nowrap;
  }

</style>
