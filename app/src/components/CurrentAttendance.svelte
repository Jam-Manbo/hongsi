<script lang="ts">
  import { onMount } from 'svelte';
  import { ApiError, api } from '../lib/api';
  import { POLL_MS, afterSubmit, checkNow, classWatch, isAttended, lectureMark, markFor, markTitle, nextClass, sessionState, useClassWatch } from '../lib/classwatch.svelte';
  import { ago } from '../lib/format';
  import { errorText, writeBlocked } from '../lib/net.svelte';
  import { attendance, attendanceReceipts, handleAuthError, lectures, timetable } from '../lib/store.svelte';
  import { toast } from '../lib/ui.svelte';
  import { refreshState, refreshTab } from '../lib/refresh.svelte';
  import type { ActiveLecture } from '../lib/types';
  import Icon from './Icon.svelte';
  import LoadError from './LoadError.svelte';
  import Ring from './Ring.svelte';
  import Sheet from './Sheet.svelte';
  import Skeleton from './Skeleton.svelte';

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
  const openCount = $derived(openList.filter((l) => !isAttended(lectureMark(l))).length);
  const current = $derived(classWatch.current);
  const currentMark = $derived(current ? markFor(current) : null);
  const watching = $derived(current && !currentMark ? current : null);
  const left = $derived(Math.max(0, Math.min(1, (classWatch.nextAt - classWatch.now) / POLL_MS)));
  const headline = $derived.by(() => {
    if (!lectures.data && !lectures.error) return '출석 가능한 수업을 확인하고 있어요';
    if (lectures.error && !openCount && !currentMark) return '출석 정보를 확인해 주세요';
    if (openCount) return '출석할 수 있어요';
    if (watching) return sessionState(watching).label === '확인 불가'
      ? `${watching.name} · 출석 확인 불가` : `${watching.name} 출석을 기다리는 중`;
    if (current && currentMark) return `${current.name} · ${markTitle(currentMark)}`;
    return openList.length ? '출석 완료' : '출석할 수업이 없어요';
  });
  const next = $derived(nextClass(classWatch.now));

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
      geo = { state: 'error', msg: '이 기기에서는 위치를 확인할 수 없어요' };
      return;
    }
    geo = { state: 'finding' };
    navigator.geolocation.getCurrentPosition(
      (p) => { if (request === geoRequest) geo = { state: 'ok', lat: p.coords.latitude, lon: p.coords.longitude, acc: Math.round(p.coords.accuracy) }; },
      (e) => { if (request === geoRequest) geo = { state: 'error', msg: e.code === 1 ? '위치 권한을 허용해 주세요' : '위치를 찾지 못했어요' }; },
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
      const confirmed = afterSubmit(res);
      const wrongCode = /(?:출결|인증|출석)\s*번호/.test(res.message) && /일치하지|불일치|틀|잘못/.test(res.message);
      const rejected = wrongCode || /실패|오류|에러|틀렸|틀립|잘못|불일치|일치하지|만료|초과|불가|벗어|못했|못하/.test(res.message);
      if (confirmed) {
        open = false;
        code = '';
        toast('출석확인이 완료되었습니다.', 'success', 5000);
      } else {
        result = wrongCode ? '출결번호가 일치하지 않습니다.' : rejected ? res.message : '학교 응답을 처리하지 못했습니다.';
      }
      lectures.load(true);
      attendance.load(true);
    } catch (err) {
      if (!handleAuthError(err)) result = err instanceof ApiError && (err.status === 0 || err.status >= 500)
        ? '학교 응답을 처리하지 못했습니다.'
        : errorText(err, '학교 응답을 처리하지 못했습니다.');
    } finally {
      busy = false;
    }
  }
</script>

  <section class="now card" class:watching={!!watching} aria-label="지금 출석 상태">
    <div class="now-head">
      <div>
        {#if showLabel}<span class="eyebrow">{watching ? `지금 출석 · ${watching.start} 수업` : '지금 출석'}</span>{/if}
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
    {#if lectures.error}<p class="muted small" role="status">{lectures.error}</p>{/if}
    <LoadError resource={attendanceReceipts} what="공유된 출석 기록을" />
    {#if classWatch.shareError}<p class="muted small" role="status">{classWatch.shareError}</p>{/if}
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
              <button class="btn btn-primary" onclick={() => startAttend(l)} disabled={busy}><Icon name="check" size={18} />출석하기</button>
            {/if}
          </div>
        {/each}
      </div>
    {:else if lectures.data && !lectures.error}
      <div class="watch-row">
        <p class="muted small" aria-live="polite">
          {#if watching}
            {sessionState(watching).label === '확인 불가' ? '확인된 출석 결과가 없어요.' : classWatch.polling ? '확인하는 중…' : `수업 시작 3분 전부터 10분 뒤까지 ${POLL_MS / 1000}초마다 자동으로 확인해요`}
          {:else if current && currentMark}
            {current.start} 수업
          {:else if next}
            다음 수업 {next.start} · {next.name}
          {:else if lectures.at}
            {ago(lectures.at / 1000)} 확인
          {/if}
        </p>
      </div>
    {:else if !lectures.error}
      <Skeleton rows={1} height={44} />
    {/if}
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
    gap: 12px;
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

  .now-head > div { min-width: 0; }
  .now-head > .icon-btn { flex: none; }
  .lecture strong { overflow-wrap: anywhere; }

  @media (max-width: 359px) {
    .now { padding: 14px; }
    .lecture { padding: 10px; gap: 8px; }
    .lecture .btn { padding: 10px; gap: 4px; }
  }
</style>
