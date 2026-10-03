<script lang="ts">
  import { isApp } from '../lib/api';
  import { appUpdater, canUpdateApp } from '../lib/app-update.svelte';
  import {
    setMealPlace,
    setTimetableDisplay,
    setShowUndatedAssignments,
    setMidnight,
    setTheme,
    settings,
    type MealPlace,
    type Midnight,
    type Theme,
  } from '../lib/settings.svelte';
  import NotificationSettings from './NotificationSettings.svelte';
  import { app, pref } from '../lib/store.svelte';
  import Avatar from './Avatar.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import Switch from './Switch.svelte';

  let { open = $bindable(false), onlogout, onlogoutall }: { open: boolean; onlogout: () => Promise<void>; onlogoutall: () => Promise<void> } = $props();
  let confirmLogoutAll = $state(false);

  $effect(() => { if (!open) confirmLogoutAll = false; });

  const THEMES: { id: Theme; label: string }[] = [
    { id: 'system', label: '시스템' },
    { id: 'light', label: '라이트' },
    { id: 'dark', label: '다크' },
  ];
  const MIDNIGHT: { id: Midnight; label: string; hint: string }[] = [
    { id: 'prev', label: '전날 24:00', hint: '10/3 00:00 마감 → 10/2 24:00' },
    { id: 'same', label: '그날 00:00', hint: '클래스룸과 같게 10/3 00:00' },
  ];

  const PLACES: { id: MealPlace; label: string }[] = [
    { id: 'dorm', label: '제2기숙사' },
    { id: 'staff', label: '교직원식당' },
  ];

  const studentId = $derived(app.profile?.studentId || pref('last-id', ''));

</script>

<Sheet bind:open title="내 정보 · 설정">
  <div class="me">
    <Avatar size={64} />
    <div>
      <h3>{app.profile?.name || '내 정보'}</h3>
      <p class="muted">{[studentId, app.profile?.department].filter(Boolean).join(' · ')}</p>
      <span class="chip {app.remembered ? 'ok' : ''}">
        <Icon name={app.remembered ? 'tick' : 'clock'} size={13} stroke={2.4} />
        {#if isApp}{app.remembered ? '자동 로그인 켜짐' : '자동 로그인 꺼짐'}{:else}{app.remembered ? '로그인 상태 유지 중' : '로그인 상태 유지 꺼짐'}{/if}
      </span>
    </div>
  </div>

  <h4 class="section-title">설정</h4>
  <div class="settings card">
    <div class="set">
      <span class="set-label">화면 테마</span>
      <div class="seg" role="radiogroup" aria-label="화면 테마">
        {#each THEMES as t (t.id)}
          <button role="radio" aria-checked={settings.theme === t.id} class:on={settings.theme === t.id} onclick={() => setTheme(t.id)}>{t.label}</button>
        {/each}
      </div>
    </div>
    <div class="set">
      <span class="set-label">00:00 마감 표시</span>
      <div class="seg" role="radiogroup" aria-label="00:00 마감 표시">
        {#each MIDNIGHT as m (m.id)}
          <button role="radio" aria-checked={settings.midnight === m.id} class:on={settings.midnight === m.id} onclick={() => setMidnight(m.id)}>{m.label}</button>
        {/each}
      </div>
      <p class="set-hint muted">{MIDNIGHT.find((m) => m.id === settings.midnight)?.hint}</p>
    </div>
    <div class="set">
      <span class="set-label">학식 기본 식당</span>
      <div class="seg" role="radiogroup" aria-label="학식 기본 식당">
        {#each PLACES as m (m.id)}
          <button role="radio" aria-checked={settings.mealPlace === m.id} class:on={settings.mealPlace === m.id} onclick={() => setMealPlace(m.id)}>{m.label}</button>
        {/each}
      </div>
    </div>
    <div class="set">
      <span class="set-label">주간 시간표</span>
      <div class="seg" role="radiogroup" aria-label="주간 시간표">
        <button role="radio" aria-checked={settings.timetableDisplay === 'full'} class:on={settings.timetableDisplay === 'full'} onclick={() => setTimetableDisplay('full')}>전체 표시</button>
        <button role="radio" aria-checked={settings.timetableDisplay === 'fit'} class:on={settings.timetableDisplay === 'fit'} onclick={() => setTimetableDisplay('fit')}>최적화 표시</button>
      </div>
      <p class="set-hint muted">{settings.timetableDisplay === 'full'
        ? '앞뒤 공강을 포함하여 전체 시간표를 표시합니다.'
        : '앞뒤 공강이 있을 경우 해당 시간을 제외하고 시간표를 표시합니다.'}</p>
    </div>
    <div class="set">
      <span class="set-label">마감일 없는 과제 표시<Switch checked={settings.showUndatedAssignments} label="마감일 없는 과제 표시" onchange={setShowUndatedAssignments} /></span>
    </div>
  </div>

  <h4 class="section-title">알림</h4>
  <div class="settings card"><NotificationSettings /></div>

  {#if canUpdateApp}
    <h4 class="section-title">앱 업데이트</h4>
    <div class="settings card">
      <div class="set-label">
        <span>홍시 {appUpdater.currentVersion}</span>
        <button class="filter" disabled={appUpdater.checking || appUpdater.installing} onclick={() => appUpdater.check(true)}>{appUpdater.checking ? '확인 중…' : '업데이트 확인'}</button>
      </div>
    </div>
  {/if}


  <h4 class="section-title">계정</h4>
  <div class="settings card">
    <div class="set">
      <button class="btn btn-danger btn-block" disabled={app.loggingOut} onclick={() => confirmLogoutAll = true}><Icon name="logout" size={18} />모든 기기에서 로그아웃</button>
      <p class="set-hint muted">모든 기기에서 로그아웃하고 저장된 로그인 정보를 모두 삭제해요.</p>
    </div>
  </div>


  {#snippet footer()}
    <button class="btn btn-danger btn-block" disabled={app.loggingOut} onclick={onlogout}><Icon name="logout" size={18} />{app.loggingOut ? '로그아웃하는 중…' : '로그아웃'}</button>
  {/snippet}
</Sheet>

<Sheet bind:open={confirmLogoutAll} title="모든 기기에서 로그아웃할까요?" layer={1}>
  <div class="logout-confirm">
    <p>모든 기기에서 로그아웃하고 저장된 로그인 정보를 모두 삭제해요.</p>
    <p>서버에 저장된 모든 로그인 정보와 백그라운드 동기화·푸시 알림 등록 정보를 삭제합니다. 오프라인인 기기에 저장된 로그인 정보는 해당 기기가 다시 서버에 연결될 때 삭제합니다.</p>
  </div>
  {#snippet footer()}
    <button class="btn btn-ghost w1" disabled={app.loggingOut} onclick={() => confirmLogoutAll = false}>취소</button>
    <button class="btn btn-danger w2" disabled={app.loggingOut} onclick={onlogoutall}>{app.loggingOut ? '로그아웃하는 중…' : '모든 기기에서 로그아웃'}</button>
  {/snippet}
</Sheet>

<style>
  .me {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 20px 16px;
    background: var(--surface-2);
    border-radius: var(--radius);
  }

  .me div {
    display: grid;
    gap: 3px;
    justify-items: start;
  }

  h3 {
    font-size: 20px;
    font-weight: 800;
    letter-spacing: -0.02em;
  }

  .me p {
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }

  .logout-confirm { display: grid; gap: 12px; }
  .logout-confirm p { font-size: 14px; line-height: 1.7; color: var(--text-2); }

  h4.section-title {
    margin-top: 22px;
  }

  .settings {
    display: grid;
    gap: 20px;
    padding: 18px 16px;
  }

  .set {
    display: grid;
    gap: 8px;
  }

  .set-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 14px;
    font-weight: 650;
    color: var(--text-2);
  }

  .set-hint {
    font-size: 12px;
  }

  .seg {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-3);
  }

  .seg button {
    min-height: 40px;
    border-radius: 9px;
    font-size: 13.5px;
    font-weight: 650;
    color: var(--text-2);
  }

  .seg button.on {
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow-sm);
  }

  .me > div { min-width: 0; }
  .me p { overflow-wrap: anywhere; }
  .set + .set { padding-top: 18px; border-top: 1px solid var(--border); }
  .set-label { color: var(--text); }
  .set-hint { line-height: 1.65; }
  @media (max-width: 380px) { .me { gap: 12px; padding-inline: 12px; } .me p { font-size: 12px; } }

</style>
