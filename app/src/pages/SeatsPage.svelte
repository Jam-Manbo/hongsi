<script lang="ts">
  import { onMount } from 'svelte';
  import { api, isApp } from '../shared/api/api';
  import { ago, time } from '../shared/utils/format';
  import { errorText, writeBlocked } from '../shared/api/net.svelte';
    import { PERIODS, seatLabel, seatPrefs, setPeriod, syncSeatReminders } from '../features/seats/seat.svelte';
  import { handleAuthError } from '../features/auth/auth-state.svelte';
  import { seatSession, seats } from '../features/seats/seat-resources.svelte';
  import { isCurrentSession, sessionVersion } from '../shared/state/session';
  import { focus, toast, toastOnce } from '../shared/state/ui.svelte';
  import type { RecentSeat, Room, SeatCell } from '../shared/types';
  import Icon from '../shared/ui/Icon.svelte';
  import LoadError from '../shared/ui/LoadError.svelte';
  import MySeat from '../features/seats/MySeat.svelte';
  import SeatMap from '../features/seats/SeatMap.svelte';
  import SeatLegend from '../features/seats/SeatLegend.svelte';
  import Sheet from '../shared/ui/Sheet.svelte';
  import Skeleton from '../shared/ui/Skeleton.svelte';

  let building = $state('T');
  $effect(() => {
    if (focus.seatBuilding) {
      building = focus.seatBuilding;
      roomNo = 0;
      focus.seatBuilding = null;
    }
  });
  const DONG: Record<string, string> = { G: 'G동', T: 'T동', R: 'R동' };
  let roomNo = $state(0);
  let recent = $state<RecentSeat[]>([]);
  let picked = $state<SeatCell | null>(null);
  let sheetOpen = $state(false);
  let busy = $state(false);

  onMount(() => {
    seats.load();
    seatSession.load();
    const t = setInterval(() => seats.load(true), 60_000);
    return () => clearInterval(t);
  });

  const data = $derived(seats.data);
  const session = $derived(seatSession.data?.session ?? null);
  const current = $derived(data?.buildings.find((b) => b.id === building) ?? data?.buildings[0]);
  const room = $derived<Room | undefined>(current?.rooms.find((r) => r.no === roomNo) ?? current?.rooms[0]);
  const mine = $derived(session && room && session.building === current?.id && session.roomNo === room.no ? session.seatNo : null);
  const recentSet = $derived(new Set(recent.map((r) => r.seatNo)));

  $effect(() => {
    if (!current || !room) return;
    const [b, r] = [current.id, room.no];
    const load = () =>
      api
        .recentSeats(b, r)
        .then((list) => (recent = list))
        .catch(() => (recent = []));
    load();
    const t = setInterval(load, 60_000);
    return () => clearInterval(t);
  });

  function selectBuilding(id: string) {
    building = id;
    roomNo = 0;
  }

  function selectRoom(no: number) {
    roomNo = no;
  }

  function pick(no: number) {
    picked = room?.grid.flat().find((c) => c?.no === no) ?? null;
    sheetOpen = picked !== null;
  }

  async function checkIn() {
    const version = sessionVersion();
    if (!current || !room || !picked) return;
    if (writeBlocked('sync', '입실할')) return;
    busy = true;
    try {
      const res = await api.checkIn(current.id, room.no, picked.no, seatPrefs.period);
      seatSession.set({ session: res.session });
      sheetOpen = false;
      if (!isCurrentSession(version)) return;
      syncSeatReminders(res.session);
      toast(
        `입실 완료 · ${time(res.session.expiresAt)}까지 이용 가능`,
        'success',
        4500,
      );
    } catch (e) {
      if (!handleAuthError(e)) toastOnce(errorText(e, '입실하지 못했어요.'), 'error');
    } finally {
      busy = false;
    }
  }

  const pct = (r: Room) => (r.total ? Math.round(((r.total - r.free) / r.total) * 100) : 0);
  const recentFor = (no: number) => recent.find((r) => r.seatNo === no);
</script>

<div class="page">
  {#if session}
    <MySeat {session} />
  {/if}

  <LoadError resource={seats} what="열람실 좌석을" />

  {#if !data || !current}
    {#if !seats.error}
      <div style="height: 14px"></div>
      <Skeleton rows={1} height={48} />
      <div style="height: 10px"></div>
      <Skeleton rows={1} height={320} />
    {/if}
  {:else}
    <div class="buildings" role="tablist" aria-label="건물">
      {#each data.buildings as b (b.id)}
        <button role="tab" class="bld" aria-selected={b.id === current.id} onclick={() => selectBuilding(b.id)}>
          <strong class="building-name">{b.name}<small class="dong">({DONG[b.id] ?? b.id})</small></strong>
        </button>
      {/each}
    </div>

    <div class="rooms">
      {#each current.rooms as r (r.no)}
        <button class="room card" class:on={room?.no === r.no} onclick={() => selectRoom(r.no)} aria-pressed={room?.no === r.no}>
          <span class="name">{r.name}</span>
          <span class="count"><strong>{r.free}</strong> / {r.total} 빈 자리</span>
          <span class="bar" aria-hidden="true"><i style:width="{pct(r)}%" class:hot={pct(r) > 85}></i></span>
          {#if session && session.building === current.id && session.roomNo === r.no}<span class="chip warn mine">내 자리</span>{/if}
        </button>
      {/each}
    </div>

    {#if room}
      <div class="map-head" class:stack={isApp}>
        <h2><span class="building-name">{current.name}<small class="dong">({DONG[current.id] ?? current.id})</small></span> {room.name}</h2>
        <SeatLegend />
        <span class="updated muted">
          {ago(data.fetchedAt)} 업데이트
          {#if seats.loading}<span class="spin inline"><Icon name="refresh" size={14} /></span>{/if}
        </span>
      </div>
      {#if recent.length && !session}
        <div class="recent">
          <Icon name="sparkle" size={16} />
          <span>방금 배정된 자리</span>
          {#each recent.slice(0, 6) as r (r.seatNo)}
            <button class="chip warn" onclick={() => pick(r.seatNo)}>{r.seatNo}번 · {time(r.at)}</button>
          {/each}
        </div>
      {/if}
      <SeatMap {room} {mine} recent={recentSet} selected={picked?.no ?? null} onpick={pick} />
    {/if}
  {/if}
</div>

<Sheet bind:open={sheetOpen} title={picked && room ? `${room.name} ${picked.no}번` : '좌석'} onclose={() => (picked = null)}>
  {#if picked && current && room}
    {@const r = recentFor(picked.no)}
    <div class="seat-info">
      <span class="chip {picked.state === 'free' ? 'info' : 'muted'}">{picked.state === 'free' ? '빈 자리' : '사용 중'}</span>
      {#if r}<span class="chip warn">{time(r.at)}에 배정됨</span>{/if}
    </div>
    {#if session}
      <p class="sheet-text">이미 <b>{seatLabel(session)}</b>에 입실 중이에요.<br />자리를 옮겼다면 먼저 퇴실해 주세요.</p>
    {:else}
      <p class="sheet-text">
        좌석배정기에서 이 자리를 받았다면 입실을 눌러 주세요.<br />
        {#if picked.state === 'used'}배정 시각은 좌석 지도 기록에서 자동으로 찾아요.{:else}지도에 아직 반영되지 않았다면 누른 시각으로 기록해요.{/if}
      </p>
      <div class="field">
        <span>좌석 이용 시간</span>
        <div class="filters">
          {#each PERIODS as p (p.id)}
            <button class="filter" aria-pressed={seatPrefs.period === p.id} onclick={() => setPeriod(p.id)}>
              {p.label} {p.hours}시간
            </button>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
  {#snippet footer()}
    <button class="btn btn-ghost w1" onclick={() => (sheetOpen = false)}>닫기</button>
    {#if !session}
      <button class="btn btn-primary w2" onclick={checkIn} disabled={busy}>
        <Icon name="enter" size={18} />{busy ? '입실 중…' : '이 자리로 입실'}
      </button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .buildings {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    margin: 18px 0 10px;
    padding: 4px;
    border-radius: 16px;
    background: var(--surface-3);
  }

  .bld {
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 48px;
    padding: 8px 4px;
    border-radius: 12px;
    text-align: center;
    color: var(--text-2);
    transition: background 0.15s, color 0.15s;
  }

  .bld strong {
    font-size: clamp(14px, 1.4vw, 20px);
    line-height: 1.3;
    font-weight: 750;
  }

  .building-name { display: inline-flex; align-items: baseline; gap: 2px; white-space: nowrap; }
  .dong { font-size: 0.72em; font-weight: 550; color: var(--text-3); }

  .bld[aria-selected='true'] {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-sm);
  }

  .rooms {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }

  .room {
    position: relative;
    display: grid;
    gap: 6px;
    padding: 16px;
    text-align: left;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }

  .room.on {
    border-color: var(--primary);
    box-shadow: none;
    background: var(--primary-weak);
  }

  .name {
    font-size: 14px;
    font-weight: 700;
  }

  .count {
    font-size: 12.5px;
    color: var(--text-3);
  }

  .count strong {
    font-size: 23px;
    font-weight: 750;
    color: var(--seat-free);
    font-variant-numeric: tabular-nums;
  }

  .bar {
    height: 6px;
    border-radius: 999px;
    background: var(--surface-3);
    overflow: hidden;
  }

  .bar i {
    display: block;
    height: 100%;
    border-radius: 999px;
    background: var(--primary);
  }

  .bar i.hot {
    background: var(--danger);
  }

  .mine {
    position: absolute;
    top: 10px;
    right: 10px;
  }

  .map-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 18px;
    margin: 20px 4px 10px;
  }

  .map-head h2 {
    font-size: 16px;
    font-weight: 750;
  }

  .map-head .updated {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12.5px;
  }

  .map-head.stack {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .map-head.stack :global(.legend) {
    grid-column: 1 / -1;
    order: 3;
  }

  @media (max-width: 639px) {
    .map-head {
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
    }

    .map-head :global(.legend) {
      grid-column: 1 / -1;
      order: 3;
    }
  }

  .inline {
    display: inline-flex;
  }

  .recent {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0 4px 10px;
    font-size: 13px;
    font-weight: 650;
    color: var(--warn);
  }

  .seat-info {
    display: flex;
    gap: 6px;
    margin-bottom: 10px;
  }

  .sheet-text {
    color: var(--text-2);
    font-size: 14.5px;
    margin-bottom: 14px;
  }

  .field {
    display: grid;
    gap: 8px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }
</style>
