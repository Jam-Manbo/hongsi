<script lang="ts">
  import { onMount } from 'svelte';
  import { ALERT_LEADS, settings, toggleAlertLead } from '../lib/settings.svelte';
  import { notificationsAllowed, notificationState as notifications, setNotificationsEnabled } from '../lib/notify';
  import { background, disableBackground, setBackgroundEnabled, setBackgroundAlerts, refreshBackground, retryBackgroundPermission } from '../lib/background.svelte';
  import Switch from './Switch.svelte';
  import Icon from './Icon.svelte';

  const permissionText = { unknown: '권한 확인 중', granted: '', denied: '알림 권한이 꺼져 있어요', default: '알림 권한이 설정되지 않았어요', unsupported: '알림을 지원하지 않는 환경이에요' };
  const local = $derived([notifications.due, notifications.seat]);
  let busy = $state(false);
  let checking = $state(false);
  async function checkBackground() {
    if (checking) return;
    checking = true;
    try { await refreshBackground(); } finally { checking = false; }
  }
  onMount(() => { void notificationsAllowed(); void checkBackground(); });
  async function toggleDevice() {
    if (busy) return;
    busy = true;
    try {
      const enabled = !notifications.enabled;
      const stopped = enabled ? Promise.resolve() : disableBackground();
      await setNotificationsEnabled(enabled);
      await stopped;
    } finally { busy = false; }
  }
  function lead(min: number) { toggleAlertLead(min); if (settings.alertLeads.length && notifications.permission !== 'granted') void notificationsAllowed(true); }
</script>

<div class="notification-settings">
  <div class="setting-head">
    <div><strong>이 기기 알림</strong>{#if notifications.permission !== 'granted'}<p class="hint">{permissionText[notifications.permission]}</p>{/if}</div>
    {#if notifications.permission === 'granted'}
      <Switch checked={notifications.enabled} label="이 기기 알림" disabled={busy} onchange={toggleDevice} />
    {:else if notifications.permission !== 'unsupported'}
      <button class="filter" disabled={notifications.permission === 'unknown'} onclick={retryBackgroundPermission}>권한 설정</button>
    {/if}
  </div>
  {#if notifications.permission === 'denied'}<p class="hint">브라우저·기기의 알림 설정에서 홍시를 허용해 주세요.</p>{/if}
  <strong class="label">마감 알림</strong>
  <div class="leads" role="group" aria-label="마감 알림 시간">
    {#each ALERT_LEADS as l (l.min)}<button class="filter" aria-pressed={settings.alertLeads.includes(l.min)} onclick={() => lead(l.min)}>{l.label}</button>{/each}
  </div>
  {#each [...new Set([notifications.error, ...local.map((c) => c.error)].filter(Boolean))] as error}<p class="error" role="alert">{error}</p>{/each}
  <div class="background">
    <div class="setting-head">
      <strong>백그라운드 일정 확인</strong>
      <Switch checked={background.choice === true} label="백그라운드 일정 확인" onchange={setBackgroundEnabled} />
    </div>
    <p class="hint">학교 로그인 세션을 서버에 암호화해 보관하고, {background.status?.pollMinutes ?? 5}분마다 클래스룸 알림과 과제·강의 변경을 확인해요. 비밀번호는 서버에 저장하지 않아요.</p>
    {#if background.busy}<p class="hint" role="status">설정을 적용하고 있어요.</p>{/if}
    <div class="change-alerts">
      <div class="setting-head">
        <strong>백그라운드 알림</strong>
        <Switch checked={background.choice === true && background.changeAlerts} label="백그라운드 알림" disabled={background.choice !== true} onchange={setBackgroundAlerts} />
      </div>
      <p class="hint">클래스룸에서 새 알림이 생기거나, 과제, 강의, 자료 등 정보가 추가 혹은 수정되었을 때 알림을 보냅니다</p>
      {#if background.choice !== true}<p class="hint">백그라운드 일정 확인을 켜면 적용돼요.</p>{/if}
    </div>
    {#if background.error}
      <div class="background-status">
        <p class="error" role="alert">{background.error}</p>
        <button class="icon-btn" aria-label="백그라운드 설정 다시 적용" disabled={checking || background.busy} onclick={checkBackground}><Icon name="refresh" size={18} /></button>
      </div>
    {/if}
  </div>
</div>
<style>
  .notification-settings { display: grid; gap: 12px; }
  .setting-head { display:flex;align-items:center;justify-content:space-between;gap:8px; }
  .setting-head strong,.label,.background strong {font-size:14px;}
  .hint {font-size:13px;color:var(--text-2);line-height:1.6;}
  .leads {display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:6px;}
  .leads .filter {padding:0 4px;}
  .error {font-size:13px;color:var(--danger);line-height:1.5;}
  .background-status {display:flex;align-items:center;gap:6px;}
  .background-status .icon-btn {flex:none;}
  .change-alerts {display:grid;gap:8px;margin-top:12px;}
  .background {border-top:1px solid var(--border);padding-top:20px;display:grid;gap:8px;}
</style>
