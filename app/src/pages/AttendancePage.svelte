<script lang="ts">
  import { onMount } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { api, isApp } from '../lib/api';
  import {
    POLL_MS,
    afterSubmit,
    checkNow,
    classWatch,
    classSessions,
    inWindow,
    isAttended,
    lectureMark,
    markFor,
    markTitle,
    periodLabel,
    sourceSuffix,
    todayClasses,
    useClassWatch,
    type TodayClass,
  } from '../lib/classwatch.svelte';
  import { ago, WEEKDAYS } from '../lib/format';
  import { courseColors } from '../lib/colors';
  import { errorText, writeBlocked } from '../lib/net.svelte';
  import { attendance, calendar, handleAuthError, lectures, pref, setPref, timetable } from '../lib/store.svelte';
  import { focus, toast } from '../lib/ui.svelte';
  import { refreshState, refreshTab } from '../lib/refresh.svelte';
  import type { ActiveLecture, AttendanceMark, AttendanceWeek, MarkKind } from '../lib/types';
  import Icon from '../components/Icon.svelte';
  import LoadError from '../components/LoadError.svelte';
  import Popover from '../components/Popover.svelte';
  import Ring from '../components/Ring.svelte';
  import Sheet from '../components/Sheet.svelte';
  import Skeleton from '../components/Skeleton.svelte';
  import EmptyState from '../components/EmptyState.svelte';
  import WeekTimetable from '../components/WeekTimetable.svelte';

  let target = $state<ActiveLecture | null>(null);
  let open = $state(false);
  let code = $state('');
  let geo = $state<{ state: 'idle' | 'finding' | 'ok' | 'error'; lat?: number; lon?: number; acc?: number; msg?: string }>({
    state: 'idle',
  });
  let busy = $state(false);
  let result = $state('');
  const submittedMark = $derived(target ? lectureMark(target) : null);
  const submitted = $derived(isAttended(submittedMark));

  const phone = new MediaQuery('max-width: 639px');
  const desktop = new MediaQuery('min-width: 1024px');
  let view = $state<'today' | 'week'>(pref('attendance-view', 'today'));
  let weekOpen = $state(false);
  const showToday = $derived(desktop.current || phone.current || view === 'today');
  const weekInline = $derived(desktop.current || (!phone.current && view === 'week'));

  function setView(v: 'today' | 'week') {
    view = v;
    setPref('attendance-view', v);
  }

  function openWeek() {
    if (phone.current) weekOpen = true;
    else if (!desktop.current) setView('week');
  }

  onMount(() => {
    attendance.load();
    calendar.load();
    timetable.load();
    if (focus.timetable) {
      focus.timetable = false;
      openWeek();
    }
    return useClassWatch();
  });

  const openList = $derived(lectures.at > classWatch.now - 10 * 60_000 ? (lectures.data?.items ?? []) : []);
  const openCount = $derived(openList.filter((l) => !isAttended(lectureMark(l))).length);
  const current = $derived(classWatch.current);
  const currentMark = $derived(current ? markFor(current) : null);
  const currentDone = $derived(isAttended(currentMark));
  const watching = $derived(current && !currentDone ? current : null);
  const left = $derived(Math.max(0, Math.min(1, (classWatch.nextAt - classWatch.now) / POLL_MS)));
  const headline = $derived(
    openCount
      ? `출석할 수 있는 수업 ${openCount}개`
      : watching
        ? `${watching.name} 출석을 기다리는 중`
        : current && currentDone && currentMark
          ? `${current.name} · ${markTitle(currentMark)}`
          : openList.length ? '출석 완료' : '출석할 수업이 없어요',
  );
  const today = $derived(todayClasses(timetable.data?.slots ?? [], classWatch.now));

  function classState(c: TodayClass): { label: string; cls: string } | null {
    const m = markFor(c);
    if (isAttended(m)) return { label: m.label, cls: m.kind === 'late' ? 'warn' : 'ok' };
    if (m?.kind === 'absent') return { label: '결석', cls: 'danger' };
    if (inWindow(c, classWatch.now)) return { label: '확인 중', cls: 'primary' };
    if (c.at > classWatch.now) return { label: '예정', cls: '' };
    return { label: '확인 불가', cls: '' };
  }

  const colorByCode = $derived.by(() => {
    const courses = calendar.data?.courses ?? [];
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

  function startAttend(l: ActiveLecture) {
    target = l;
    code = '';
    result = '';
    open = true;
    locate();
  }

  function locate() {
    if (!('geolocation' in navigator)) {
      geo = { state: 'error', msg: '이 기기에서는 위치를 확인할 수 없어요' };
      return;
    }
    geo = { state: 'finding' };
    navigator.geolocation.getCurrentPosition(
      (p) => (geo = { state: 'ok', lat: p.coords.latitude, lon: p.coords.longitude, acc: Math.round(p.coords.accuracy) }),
      (e) => (geo = { state: 'error', msg: e.code === 1 ? '위치 권한을 허용해 주세요' : '위치를 찾지 못했어요' }),
      { enableHighAccuracy: true, timeout: 12_000, maximumAge: 30_000 },
    );
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!target || busy || submitted || geo.state !== 'ok' || !code.trim()) return;
    if (writeBlocked('school', '출석을 보낼')) return;
    result = '';
    busy = true;
    try {
      const res = await api.submitAttendance(target.key, code.trim(), geo.lat!, geo.lon!);
      const confirmed = afterSubmit(target, res.message);
      const wrongCode = /(?:출결|인증|출석)\s*번호/.test(res.message) && /일치하지|불일치|틀|잘못/.test(res.message);
      const rejected = wrongCode || /실패|오류|에러|틀렸|틀립|잘못|불일치|일치하지|만료|초과|불가|벗어|못했|못하/.test(res.message);
      toast(wrongCode ? '출결번호가 일치하지 않습니다.' : res.message, confirmed ? 'success' : rejected ? 'error' : 'info', 5000);
      lectures.load(true);
      attendance.load(true);
    } catch (err) {
      if (!handleAuthError(err)) result = errorText(err, '출석을 보내지 못했어요');
    } finally {
      busy = false;
    }
  }
</script>

<div class="page">
  <div class="top-row">
  <div class="left-col">
  <section class="now card" class:watching={!!watching}>
    <div class="now-head">
      <div>
        <span class="eyebrow">{watching ? `지금 출석 · ${watching.start} 수업` : '지금 출석'}</span>
        <h2>{headline}</h2>
      </div>
      {#if watching}
        <button class="icon-btn ring-btn" onclick={checkNow} aria-label="지금 다시 확인" disabled={classWatch.polling}>
          <Ring value={left} size={40} stroke={3.5}>
            <span class="ring-icon" class:spin={classWatch.polling}><Icon name="refresh" size={18} stroke={2.2} /></span>
          </Ring>
        </button>
      {:else}
        <button class="icon-btn" onclick={() => refreshTab('attendance')} aria-label="다시 확인" disabled={refreshState.attendance.busy}>
          <span class:spin={refreshState.attendance.busy}><Icon name="refresh" /></span>
        </button>
      {/if}
    </div>
    <LoadError resource={lectures} what="출석 가능 수업을" stale={false} />
    {#if openList.length}
      <div class="lectures">
        {#each openList as l (l.key)}
          {@const m = lectureMark(l)}
          <div class="lecture">
            <div>
              <strong>{l.name}</strong>
              <span class="muted">{l.time}</span>
            </div>
            {#if isAttended(m)}
              <span class="chip ok done-chip"><Icon name="tick" size={14} stroke={2.6} />{m.kind === 'present' ? '출석 완료' : m.label}</span>
            {:else}
              <button class="btn btn-primary" onclick={() => startAttend(l)}><Icon name="check" size={18} />출석하기</button>
            {/if}
          </div>
        {/each}
      </div>
    {:else if lectures.data}
      <div class="watch-row">
        <p class="muted small" aria-live="polite">
          {#if watching}
            {classWatch.polling ? '확인하는 중…' : '수업 시작 3분 전부터 10분 뒤까지 5초마다 자동으로 확인해요'}
          {:else if current && currentMark && currentDone}
            {current.start} 수업{sourceSuffix(currentMark)}
          {:else}
            {(lectures.data.message ?? '출석할 수업이 없어요').replace(/\.$/, '')} · {lectures.at ? ago(lectures.at / 1000) : ''} 확인
          {/if}
        </p>
      </div>
    {:else if !lectures.error}
      <Skeleton rows={1} height={44} />
    {/if}
  </section>

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
            <span class="muted">{periodLabel(c)}{c.room ? ` · ${c.room}` : ''}{markFor(c) ? sourceSuffix(markFor(c)!) : ''}</span>
          </div>
          <div class="session-tags" aria-label="교시별 출석 상태">
            {#each sessions as session (session.at)}
              {@const state = classState(session)}
              {#if state}<span class="chip {state.cls}" title="{session.start} · {periodLabel(session)}">{sessions.length > 1 ? `${session.round}회차 ` : ''}{state.label}</span>{/if}
            {/each}
          </div>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState message="오늘은 수업이 없어요" />
  {/if}
  </section>
  {/if}
  </div>

  {#if weekInline}
    <section class="week-wrap">
      {#if !desktop.current}
        <h2 class="section-title">
          <span class="title-text">주간 시간표</span>
          {@render viewSwitch()}
        </h2>
      {/if}
      {#if !showToday}<LoadError resource={timetable} what="시간표를" stale={false} />{/if}
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
              <span class="muted small">{c.code}</span>
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
                    {/each}
                  </span>
                </button>
              {/each}
            </div>
          {:else}
            <p class="notice">{c.notice ?? '교수님이 출석부를 공개하지 않았어요'}</p>
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

<Sheet bind:open={weekOpen} title="주간 시간표" wide>
  {#if timetable.data}
    <WeekTimetable slots={timetable.data.slots} colors={colorByCode} now={classWatch.now} compact />
  {:else}
    <Skeleton rows={4} height={60} />
  {/if}
</Sheet>

<Sheet bind:open title={target?.name ?? '출결하기'} titleMeta={target?.time}>
  <form id="attend" onsubmit={submit} class="attend">
    <label class="code">
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:value={code}
        inputmode="numeric"
        autocomplete="one-time-code"
        maxlength="12"
        aria-label="출결번호"
        placeholder="출결번호"
        autofocus
      />
    </label>
    <div class="geo {geo.state}">
      <Icon name="pin" size={18} />
      <span class="geo-text">
        {#if geo.state === 'finding'}현재 위치를 확인하는 중…
        {:else if geo.state === 'ok'}현재 위치 확인됨 (오차 약 {geo.acc}m)
        {:else if geo.state === 'error'}{geo.msg}
        {:else}위치 확인 전{/if}
      </span>
      {#if geo.state === 'error'}<button type="button" class="link" onclick={locate}>다시 시도</button>{/if}
    </div>
    {#if result}<p class="result" role="status">{result}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={() => (open = false)}>닫기</button>
    <button class="btn btn-primary w2" form="attend" disabled={busy || submitted || geo.state !== 'ok' || !code.trim()}>
      {submitted && submittedMark ? markTitle(submittedMark) : busy ? '보내는 중…' : '출결하기'}
    </button>
  {/snippet}
</Sheet>

<style>
  .now {
    padding: 18px;
    display: grid;
    gap: 12px;
    background: var(--surface);
  }

  .now-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
  }

  .now.watching {
    border-color: var(--primary);
    box-shadow: none;
    background: var(--primary-weak);
  }

  .now.watching .eyebrow {
    color: var(--primary-text);
  }

  .ring-btn {
    width: 44px;
    height: 44px;
  }

  .ring-btn:disabled {
    cursor: default;
  }

  .ring-icon {
    display: grid;
    color: var(--primary-text);
  }

  .done-chip {
    height: 32px;
    padding: 0 12px;
    font-size: 13px;
  }

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

  .eyebrow {
    font-size: 12.5px;
    font-weight: 700;
    color: var(--ok);
  }

  .now h2 {
    font-size: 19px;
    font-weight: 720;
    letter-spacing: -0.02em;
  }

  .watch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px 12px;
    flex-wrap: wrap;
  }

  .small {
    font-size: 12.5px;
    font-weight: 500;
  }

  .lectures {
    display: grid;
    gap: 8px;
  }

  .lecture {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .lecture div {
    display: grid;
    min-width: 0;
  }

  .lecture .btn,
  .lecture .chip {
    flex: none;
  }

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

  .attend {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 14px;
  }

  .code {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .code input {
    width: 100%;
    min-width: 0;
    height: 60px;
    padding: 0 14px;
    border-radius: 14px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    text-align: center;
    font-size: 26px;
    font-weight: 800;
    line-height: 1.2;
    letter-spacing: 0.25em;
    text-indent: 0.25em;
  }

  .code input:placeholder-shown {
    font-size: 16px;
    font-weight: 600;
    letter-spacing: 0;
    text-indent: 0;
  }

  .code input::placeholder {
    color: var(--text-3);
  }

  .code input:focus {
    outline: none;
    border-color: var(--primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent);
  }

  .geo {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface-2);
    font-size: 13.5px;
    font-weight: 600;
    color: var(--text-2);
  }

  .geo.ok {
    background: var(--ok-weak);
    color: var(--ok);
  }

  .geo.error {
    background: var(--danger-weak);
    color: var(--danger);
  }

  .geo-text {
    flex: 1;
    min-width: 0;
  }

  .geo .link {
    flex: none;
  }

  .link {
    color: inherit;
    text-decoration: underline;
    font-weight: 700;
  }

  .result {
    padding: 12px 14px;
    border-radius: 12px;
    background: var(--danger-weak);
    color: var(--danger);
    font-weight: 650;
    white-space: pre-line;
  }
</style>
