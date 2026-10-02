<script lang="ts">
  import { api } from '../lib/api';
  import { duration, time } from '../lib/format';
  import { errorText, writeBlocked } from '../lib/net.svelte';
  import { notificationsAllowed } from '../lib/notify';
  import { configureNotificationPermission } from '../lib/background.svelte';
  import { ALERT_CHOICES, PERIODS, seatLabel, seatPrefs, syncSeatReminders, toggleAlert } from '../lib/seat.svelte';
  import { handleAuthError, seatSession } from '../lib/store.svelte';
  import { isCurrentSession, sessionVersion } from '../lib/session';
  import { toast, toastOnce } from '../lib/ui.svelte';
  import type { SeatPeriod, SeatSession } from '../lib/types';
  import Icon from './Icon.svelte';
  import Ring from './Ring.svelte';
  import Sheet from './Sheet.svelte';

  let { session, compact = false }: { session: SeatSession; compact?: boolean } = $props();

  let now = $state(Date.now());
  let busy = $state<'' | 'extend' | 'end' | 'adjust'>('');
  let confirmEnd = $state(false);
  let adjustOpen = $state(false);
  let adjustTime = $state('');
  let adjustPeriod = $state<SeatPeriod>('semester');
  let notifyOk = $state(true);

  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 1000);
    notificationsAllowed().then((ok) => (notifyOk = ok));
    return () => clearInterval(t);
  });

  const total = $derived(session.validityHours * 3600_000);
  const left = $derived(session.expiresAt * 1000 - now);
  const ratio = $derived(left / total);
  const tone = $derived(left <= 0 ? 'var(--danger)' : left < 30 * 60_000 ? 'var(--seat-mine)' : 'var(--primary)');
  const sourceText = $derived(
    session.startSource === 'detected'
      ? '좌석 지도에서 배정 시각을 자동으로 찾았어요'
      : session.startSource === 'adjusted'
        ? '직접 고친 입실 시각이에요'
        : '입실 버튼을 누른 시각 기준이에요',
  );

  async function apply(action: () => Promise<{ session: SeatSession | null }>, done: string) {
    if (writeBlocked()) {
      busy = '';
      return;
    }
    try {
      const res = await action();
      seatSession.set({ session: res.session });
      syncSeatReminders(res.session);
      toast(done, 'success');
    } catch (e) {
      if (!handleAuthError(e)) toastOnce(errorText(e, '처리하지 못했어요'), 'error');
    } finally {
      busy = '';
    }
  }

  async function extend() {
    busy = 'extend';
    await apply(api.extendSeat, '연장했어요. 퇴실 알림도 다시 맞췄어요');
  }

  async function end() {
    busy = 'end';
    confirmEnd = false;
    await apply(api.checkOut, '퇴실 처리했어요. 좌석배정기에서 반납도 잊지 마세요');
  }

  function openAdjust() {
    adjustTime = time(session.startedAt);
    adjustPeriod = session.period;
    adjustOpen = true;
  }

  async function saveAdjust() {
    const [h, m] = adjustTime.split(':').map(Number);
    const base = new Date(now + 9 * 3600_000);
    let ts = Date.UTC(base.getUTCFullYear(), base.getUTCMonth(), base.getUTCDate(), h, m) - 9 * 3600_000;
    if (ts > now + 5 * 60_000) ts -= 86_400_000;
    busy = 'adjust';
    adjustOpen = false;
    await apply(() => api.adjustSeat(Math.floor(ts / 1000), adjustPeriod), '입실 시각을 고쳤어요');
  }

  async function onToggle(min: number) {
    const version = sessionVersion();
    toggleAlert(min);
    notifyOk = await notificationsAllowed();
    if (!isCurrentSession(version)) return;
    syncSeatReminders(session);
  }
</script>

<section class="card my" class:compact aria-label="내 좌석">
  <div class="top">
    <Ring value={ratio} size={compact ? 84 : 120} stroke={compact ? 8 : 10} color={tone}>
      <div class="ring-text">
        {#if left > 0}
          <strong>{duration(left)}</strong>
          <span>남음</span>
        {:else}
          <strong>만료</strong>
        {/if}
      </div>
    </Ring>
    <div class="info">
      <span class="eyebrow"><Icon name="seat" size={15} /> 입실 중</span>
      <h3>{seatLabel(session)}</h3>
      <p class="until">
        <strong>{time(session.expiresAt)}</strong> 퇴실 예정
        {#if session.extendCount > 0}<span class="chip primary">{session.extendCount}회 연장</span>{/if}
      </p>
      <p class="since">
        {time(session.startedAt)} 입실 · {PERIODS.find((p) => p.id === session.period)?.label} {session.validityHours}시간
      </p>
    </div>
  </div>

  {#if session.seatState === 'free' && now - session.startedAt * 1000 > 3 * 60_000}
    <div class="warn-box">
      <Icon name="alert" size={18} />
      <span>좌석 지도에서 이 자리가 비어 있어요.</span>
    </div>
  {/if}

  <div class="actions">
    <button class="btn btn-soft" onclick={extend} disabled={busy !== ''}>
      <Icon name="extend" size={18} />{busy === 'extend' ? '연장 중…' : '연장'}
    </button>
    <button class="btn btn-ghost" onclick={() => (confirmEnd = true)} disabled={busy !== ''}>
      <Icon name="exit" size={18} />퇴실
    </button>
  </div>

  {#if !compact}
    <div class="alerts">
      <div class="alerts-head">
        <span><Icon name="bell" size={16} /> 퇴실 알림</span>
        {#if !notifyOk}<button class="link" onclick={async () => { await configureNotificationPermission(); notifyOk = await notificationsAllowed(); }}>알림 허용하기</button>{/if}
      </div>
      <div class="filters">
        {#each ALERT_CHOICES as min (min)}
          <button class="filter" aria-pressed={seatPrefs.alerts.includes(min)} onclick={() => onToggle(min)}>
            {min === 0 ? '만료 시각' : `${min}분 전`}
          </button>
        {/each}
      </div>
    </div>
    <p class="note">
      <Icon name="sparkle" size={14} />{sourceText}.
      <button class="link" onclick={openAdjust}>시각 고치기</button>
    </p>
  {/if}
</section>

<Sheet bind:open={confirmEnd} title="퇴실할까요?">
  <p class="sheet-text">퇴실 알림이 꺼져요. 좌석배정기에서 좌석 반납도 해 주세요.</p>
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={() => (confirmEnd = false)}>취소</button>
    <button class="btn btn-danger w2" onclick={end}>퇴실하기</button>
  {/snippet}
</Sheet>

<Sheet bind:open={adjustOpen} title="입실 시각 고치기">
  <label class="field">
    <span>좌석배정기에서 배정받은 시각</span>
    <input type="time" bind:value={adjustTime} />
  </label>
  <div class="field">
    <span>좌석 유효시간</span>
    <div class="filters">
      {#each PERIODS as p (p.id)}
        <button class="filter" aria-pressed={adjustPeriod === p.id} onclick={() => (adjustPeriod = p.id)}>
          {p.label} {p.hours}시간
        </button>
      {/each}
    </div>
  </div>
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={() => (adjustOpen = false)}>취소</button>
    <button class="btn btn-primary w2" onclick={saveAdjust} disabled={!adjustTime}>저장</button>
  {/snippet}
</Sheet>

<style>
  .my {
    padding: 18px;
    display: grid;
    gap: 14px;
    background:
      radial-gradient(120% 80% at 100% 0%, color-mix(in srgb, var(--primary) 9%, transparent), transparent 60%),
      var(--surface);
  }

  .top {
    display: flex;
    align-items: center;
    gap: 18px;
  }

  .ring-text {
    display: grid;
    line-height: 1.15;
  }

  .ring-text strong {
    font-size: 17px;
    font-weight: 800;
    letter-spacing: -0.03em;
    font-variant-numeric: tabular-nums;
  }

  .compact .ring-text strong {
    font-size: 13px;
  }

  .ring-text span {
    font-size: 12px;
    color: var(--text-3);
  }

  .info {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .eyebrow {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    font-weight: 700;
    color: var(--seat-mine);
  }

  h3 {
    font-size: 18px;
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: 1.3;
  }

  .compact h3 {
    font-size: 16px;
  }

  .until {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    color: var(--text-2);
  }

  .until strong {
    font-size: 16px;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .since {
    font-size: 12.5px;
    color: var(--text-3);
  }

  .warn-box {
    display: flex;
    gap: 8px;
    align-items: center;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--warn-weak);
    color: var(--warn);
    font-size: 13.5px;
    font-weight: 600;
  }

  .actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .alerts {
    display: grid;
    gap: 8px;
    padding-top: 4px;
  }

  .alerts-head {
    display: flex;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-2);
  }

  .alerts-head span {
    display: inline-flex;
    gap: 6px;
    align-items: center;
  }

  .link {
    color: var(--primary-text);
    font-weight: 650;
    font-size: 13px;
  }

  .note {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-3);
  }

  .sheet-text {
    color: var(--text-2);
    padding-bottom: 8px;
  }

  .field {
    display: grid;
    gap: 8px;
    margin-bottom: 16px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  input[type='time'] {
    height: 48px;
    padding: 0 14px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    font-size: 18px;
    font-weight: 650;
  }
</style>
