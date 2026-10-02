<script lang="ts">
  import { isApp } from '../lib/api';
  import { appUpdater, canUpdateApp } from '../lib/app-update.svelte';
  import {
    setMealPlace,
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
        {#if isApp}{app.remembered ? '자동 로그인 켜짐' : '자동 로그인 꺼짐'}{:else}{app.remembered ? '로그인 상태 유지 중' : '이번 세션만 로그인'}{/if}
      </span>
    </div>
  </div>

  <p class="auth-hint muted">{#if isApp}{app.remembered ? '학번·비밀번호와 학교·클래스룸·홍시 로그인 정보를 이 기기의 보안 저장소에 보관해요.' : '자동 로그인 정보는 기기에 저장하지 않고, 현재 로그인 정보는 앱이 실행되는 동안만 사용해요.'}{:else}{app.remembered ? '학교 로그인 세션을 서버에 암호화해 보관하고, 브라우저 쿠키로 로그인 상태를 유지해요.' : '이번 브라우저 세션 동안 로그인 상태를 유지해요.'}{/if}</p>

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
      <span class="set-label">날짜 미정 과제 표시<Switch checked={settings.showUndatedAssignments} label="날짜 미정 과제 표시" onchange={setShowUndatedAssignments} /></span>
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
      <p class="set-hint muted">모든 기기에서 로그아웃하고 저장된 모든 인증값을 삭제합니다.</p>
    </div>
  </div>


  {#snippet footer()}
    <div class="logout-actions">
      <p class="muted">로그아웃하면 이 기기의 저장된 로그인 정보와 해당 서버 세션·동기화 등록을 삭제해요. 다른 기기의 로그인은 유지돼요.</p>
      <button class="btn btn-danger btn-block" disabled={app.loggingOut} onclick={onlogout}><Icon name="logout" size={18} />{app.loggingOut ? '로그아웃하는 중…' : '로그아웃'}</button>
    </div>
  {/snippet}
</Sheet>

<Sheet bind:open={confirmLogoutAll} title="모든 기기에서 로그아웃할까요?" layer={1}>
  <div class="logout-confirm">
    <p>모든 기기에서 로그아웃하고 저장된 모든 인증값을 삭제합니다.</p>
    <p>서버의 모든 로그인 세션과 백그라운드 인증정보·동기화·푸시 등록을 삭제합니다. 오프라인인 다른 기기에 저장된 인증정보는 해당 기기가 다음에 서버에 연결할 때 삭제합니다.</p>
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

  .auth-hint { margin-top: 12px; font-size: 12px; line-height: 1.65; }
  .logout-actions { display: grid; gap: 10px; width: 100%; }
  .logout-actions p { font-size: 12px; line-height: 1.65; }
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
