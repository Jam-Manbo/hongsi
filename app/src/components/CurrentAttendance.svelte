<script lang="ts">
  import { onMount } from 'svelte';
  import { ApiError, api } from '../lib/api';
  import { POLL_MS, afterSubmit, checkNow, classWatch, isAttended, lectureMark, lectureSession, classScheduleLabel, markFor, markTitle, nextClass, sessionState, todaySessions, useClassWatch } from '../lib/classwatch.svelte';
  import { sentenceLines } from '../lib/format';
  import { errorText, writeBlocked } from '../lib/net.svelte';
  import { attendance, attendanceReceipts, handleAuthError, lectures, timetable } from '../lib/store.svelte';
  import { toast } from '../lib/ui.svelte';
  import { refreshState, refreshTab } from '../lib/refresh.svelte';
  import type { ActiveLecture } from '../lib/types';
  import Icon from './Icon.svelte';
  import LoadError from './LoadError.svelte';
  import Ring from './Ring.svelte';
  import Sheet from './Sheet.svelte';

  let { showLabel = true }: { showLabel?: boolean } = $props();

  onMount(() => {
    timetable.load();
    return useClassWatch();
  });

  let target = $state<ActiveLecture | null>(null);
  let open = $state(false);
  let code = $state('');
  let geo = $state<{ state: 'idle' | 'finding' | 'ok' | 'error'; lat?: number; lon?: number; acc?: number; msg?: string }>({
    state: 'idle',
  });
  let busy = $state(false);
  let result = $state('');
  let geoRequest = 0;
  const submittedMark = $derived(target ? lectureMark(target) : null);
  const submitted = $derived(isAttended(submittedMark));

  const openList = $derived(lectures.at > classWatch.now - 10 * 60_000 ? (lectures.data?.items ?? []) : []);
  const lecture = $derived(openList.find((l) => !isAttended(lectureMark(l))) ?? openList[0] ?? null);
  const current = $derived(classWatch.current);
  const next = $derived(nextClass(classWatch.now));
  const currentMark = $derived(current ? markFor(current) : null);
  const mark = $derived(lecture ? lectureMark(lecture) : currentMark);
  const watching = $derived(current && !currentMark ? current : null);
  const loading = $derived(!lectures.data && !lectures.error);
  const available = $derived(!!lecture && !isAttended(mark));
  const failed = $derived(!!lectures.error && !available && !mark);
  const nextOnly = $derived(!loading && !failed && !lecture && !current && !!next);
  const unknown = $derived(!available && !mark && !!watching && sessionState(watching).label === '확인 불가');
  const waiting = $derived(!loading && !failed && !available && !mark && !!watching && !unknown);
  const accent = $derived(available || waiting);
  const slot = $derived(lecture ? lectureSession(lecture) : current ?? (nextOnly ? next : null));
  const schedule = $derived(classScheduleLabel(slot, lecture?.time));
  const title = $derived(loading ? '' : failed ? '출석 정보를 불러오지 못했어요.' : lecture?.name ?? current?.name ?? (nextOnly ? next?.name : '') ?? '');
  const badgeTone = $derived(mark?.kind === 'absent' ? 'danger' : mark?.kind === 'late' ? 'warn' : mark && isAttended(mark) ? 'ok' : '');
  const refreshing = $derived(loading || lectures.loading || classWatch.polling || refreshState.attendance.busy);
  const sessions = $derived(todaySessions(timetable.data?.slots ?? [], classWatch.now));
  const emptyMessage = $derived(loading || (!timetable.data && !timetable.error)
    ? '출석 정보를 확인하고 있어요.'
    : !timetable.data ? '출석 정보를 불러오지 못했어요.'
    : !sessions.length ? '오늘은 수업이 없어요.'
    : sessions.every((s) => classWatch.now >= s.at + 3_600_000) ? '오늘 수업이 모두 끝났어요.'
    : '출석할 수업이 없어요.');
  const left = $derived(Math.max(0, Math.min(1, (classWatch.nextAt - classWatch.now) / POLL_MS)));

  function startAttend(l: ActiveLecture) {
    if (busy) return;
    target = l;
    code = '';
    result = '';
    open = true;
    locate();
  }

  function locate() {
    const request = ++geoRequest;
    if (!('geolocation' in navigator)) {
      geo = { state: 'error', msg: '이 기기에서는 위치를 확인할 수 없어요.' };
      return;
    }
    geo = { state: 'finding' };
    navigator.geolocation.getCurrentPosition(
      (p) => { if (request === geoRequest) geo = { state: 'ok', lat: p.coords.latitude, lon: p.coords.longitude, acc: Math.round(p.coords.accuracy) }; },
      (e) => { if (request === geoRequest) geo = { state: 'error', msg: e.code === 1 ? '위치 권한을 허용해 주세요.' : '위치를 찾지 못했어요.' }; },
      { enableHighAccuracy: true, timeout: 12_000, maximumAge: 30_000 },
    );
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!target || busy || submitted || geo.state !== 'ok' || !code.trim()) return;
    if (writeBlocked('school', '출결할')) return;
    result = '';
    busy = true;
    try {
      const res = await api.submitAttendance(target.key, code.trim(), geo.lat!, geo.lon!);
      const confirmed = afterSubmit(res);
      const wrongCode = /(?:출결|인증|출석)\s*번호/.test(res.message) && /일치하지|불일치|틀|잘못/.test(res.message);
      const rejected = wrongCode || /실패|오류|에러|틀렸|틀립|잘못|불일치|일치하지|만료|초과|불가|벗어|못했|못하/.test(res.message);
      if (confirmed) {
        open = false;
        code = '';
        toast('출석 확인이 완료되었습니다.', 'success', 5000);
      } else {
        result = wrongCode ? '출결번호가 일치하지 않습니다.' : rejected ? res.message : '학교 응답을 처리하지 못했습니다.';
      }
      lectures.load(true);
      attendance.load(true);
    } catch (err) {
      if (!handleAuthError(err)) result = err instanceof ApiError && (err.status === 0 || err.status >= 500)
        ? '출석 결과를 확인하지 못했어요. 출석 상태를 확인해 주세요.'
        : errorText(err, '학교 응답을 처리하지 못했습니다.');
    } finally {
      busy = false;
    }
  }
</script>

<section class="now card" class:watching={accent} class:failed aria-label="빠른 출결 상태">
  <div class="now-head">
    <div>
      {#if showLabel}<span class="eyebrow">빠른 출결</span>{/if}
      {#if title}<h2>{title}</h2>{/if}
    </div>
    <button class="icon-btn refresh-button" class:ring-btn={accent && !!watching} onclick={() => watching ? checkNow() : refreshTab('attendance')} aria-label="출결 다시 확인" aria-busy={refreshing} disabled={refreshing}>
      {#if accent && watching}
        <Ring value={left} size={40} stroke={3}>
          <span class="refresh-icon" class:spin={refreshing}><Icon name="refresh" size={18} stroke={2.2} /></span>
        </Ring>
      {:else}
        <span class="refresh-icon" class:spin={refreshing}><Icon name="refresh" size={18} /></span>
      {/if}
    </button>
  </div>
  {#if available && lecture}
    <div class="card-bottom">
      <div class="class-info">
        <span class="available-message"><i></i>출석할 수 있어요.</span>
        {#if schedule}<span class="schedule">{schedule}</span>{/if}
      </div>
      <button class="btn btn-primary attend-button" onclick={() => lecture && startAttend(lecture)} disabled={busy}><Icon name="tick" size={17} />출석하기</button>
    </div>
  {:else if !failed && mark}
    <div class="card-bottom">
      {#if schedule}<span class="schedule">{schedule}</span>{/if}
      <span class="state-badge {badgeTone}">{#if isAttended(mark)}<Icon name={mark.kind === 'late' ? 'clock' : 'tick'} size={14} />{:else if mark.kind === 'absent'}<Icon name="x" size={14} />{/if}{markTitle(mark)}</span>
    </div>
  {:else if waiting}
    <div class="class-info">
      <span class="available-message"><i></i>출석 가능 여부 확인 중</span>
      {#if schedule}<span class="schedule">{schedule}</span>{/if}
      <span class="small muted">{POLL_MS / 1000}초마다 자동 확인</span>
    </div>
  {:else if unknown && !failed}
    <div class="card-bottom">
      {#if schedule}<span class="schedule">{schedule}</span>{/if}
      <span class="state-badge warn">확인 불가</span>
    </div>
  {:else if nextOnly}
    <div class="card-bottom">
      <span class="schedule">{schedule}</span>
      <span class="state-badge">다음 수업</span>
    </div>
  {:else if failed}
    {#if schedule}<span class="schedule">{schedule}</span>{/if}
  {:else}
    <div class="empty-state" role="status">
      <span>{emptyMessage}</span>
    </div>
  {/if}
  {#if lectures.error && !failed}<p class="error-message small sentence-message" role="status">{sentenceLines(lectures.error)}</p>{/if}
  <LoadError resource={attendanceReceipts} what="공유된 출석 기록을" />
  {#if classWatch.shareError}<p class="muted small sentence-message" role="status">{sentenceLines(classWatch.shareError)}</p>{/if}
</section>

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
    {#if result}<p class="result sentence-message" role="status">{sentenceLines(result)}</p>{/if}
  </form>
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={() => (open = false)}>닫기</button>
    <button class="btn btn-primary w2" form="attend" disabled={busy || submitted || geo.state !== 'ok' || !code.trim()}>
      {submitted && submittedMark ? markTitle(submittedMark) : busy ? '보내는 중…' : '출결하기'}
    </button>
  {/snippet}
</Sheet>

<style>
  .now { padding: 17px; display: grid; gap: 14px; background: var(--surface); }
  .now-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .now-head > div { min-width: 0; }
  .now.watching { border-color: var(--primary); box-shadow: none; background: var(--primary-weak); }
  .now.failed { border-color: color-mix(in srgb, var(--danger) 45%, var(--border)); }
  .eyebrow { display: block; font-size: 12px; font-weight: 700; color: var(--text-2); }
  .now.watching .eyebrow, .available-message { color: var(--primary-text); }
  .now h2 { margin-top: 5px; font-size: 19px; font-weight: 720; line-height: 1.4; letter-spacing: -0.02em; overflow-wrap: anywhere; }
  .refresh-button { width: 40px; height: 40px; flex: none; }
  .ring-btn { color: var(--primary-text); }
  .refresh-icon { display: grid; place-items: center; }
  .card-bottom { display: flex; align-items: center; justify-content: space-between; gap: 8px 12px; flex-wrap: wrap; }
  .class-info { display: grid; gap: 5px; min-width: 0; }
  .available-message { display: flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 600; }
  .available-message i { width: 5px; height: 5px; border-radius: 50%; background: currentColor; flex: none; }
  .schedule { font-size: 12px; color: var(--text-2); white-space: nowrap; max-width: 100%; overflow: hidden; text-overflow: ellipsis; }
  .attend-button { min-height: 44px; padding: 0 14px; font-size: 15px; border-radius: 12px; gap: 6px; flex: none; margin-left: auto; }
  .state-badge { display: inline-flex; align-items: center; gap: 5px; padding: 7px 9px; border-radius: 9px; background: color-mix(in srgb, #94a3b8 10%, transparent); color: var(--text-2); font-size: 12px; font-weight: 650; white-space: nowrap; margin-left: auto; }
  .state-badge.ok { background: var(--ok-weak); color: var(--ok); }
  .state-badge.warn { background: var(--primary-weak); color: var(--primary-text); }
  .state-badge.danger { background: var(--danger-weak); color: var(--danger); }
  .empty-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 7px; min-height: 83px; padding: 20px 10px; border: 1px dashed var(--border); border-radius: 12px; text-align: center; font-size: 13px; color: var(--text-2); }
  .small { font-size: 12.5px; font-weight: 500; }
  .error-message { color: var(--danger); }

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

  @media (max-width: 359px) {
    .now { padding: 14px; }
    .attend-button { padding: 0 10px; }
  }
</style>
