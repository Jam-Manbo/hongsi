<script lang="ts">
  import { onMount } from 'svelte';
  import { ALERT_LEADS, settings, toggleAlertLead } from '../lib/settings.svelte';
  import { notificationsAllowed, notificationState as notifications, setNotificationsEnabled } from '../lib/notify';
  import { background, disableBackground, setBackgroundEnabled, setBackgroundAlerts, refreshBackground } from '../lib/background.svelte';
  import Switch from './Switch.svelte';
  import Icon from './Icon.svelte';
  import { toast } from '../lib/ui.svelte';

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
      if (notifications.enabled && (background.status?.registered || notifications.remote) && !await disableBackground()) {
        toast(background.error, 'error');
        return;
      }
      await setNotificationsEnabled(!notifications.enabled);
    } finally { busy = false; }
  }
  function lead(min: number) { toggleAlertLead(min); if (settings.alertLeads.length && notifications.permission !== 'granted') void notificationsAllowed(true); }
</script>

<div class="notification-settings">
  <div class="setting-head">
    <div><strong>이 기기 알림</strong>{#if notifications.permission !== 'granted'}<p class="hint">{permissionText[notifications.permission]}</p>{/if}</div>
    {#if notifications.permission === 'granted'}
      <Switch checked={notifications.enabled} label="이 기기 알림" disabled={busy || background.busy || background.preferencesBusy} onchange={toggleDevice} />
    {:else if notifications.permission !== 'unsupported'}
      <button class="filter" disabled={notifications.permission === 'unknown'} onclick={() => notificationsAllowed(true)}>권한 설정</button>
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
      {#if background.status}
        <Switch checked={background.status.registered && background.status.consented} label="백그라운드 일정 확인" disabled={checking || background.busy || background.preferencesBusy} onchange={setBackgroundEnabled} />
      {:else}
        <div class="background-status"><span class="hint" role="status">{checking || !background.error ? '확인 중…' : '확인하지 못했어요'}</span><button class="icon-btn" aria-label="백그라운드 일정 확인 상태 다시 확인" disabled={checking || background.busy || background.preferencesBusy} onclick={checkBackground}><Icon name="refresh" size={18} /></button></div>
      {/if}
    </div>
    <p class="hint">학교 로그인 세션을 서버에 암호화해 보관하고, {background.status?.pollMinutes ?? 5}분마다 클래스룸 알림과 과제·강의 변경을 확인해요. 비밀번호는 서버에 저장하지 않아요.</p>
    {#if background.error && background.status}<div class="background-status"><span class="hint">마지막으로 확인한 상태예요.</span><button class="icon-btn" aria-label="백그라운드 일정 확인 상태 다시 확인" disabled={checking || background.busy || background.preferencesBusy} onclick={checkBackground}><Icon name="refresh" size={18} /></button></div>{/if}
    <div class="change-alerts">
      <div class="setting-head">
        <strong>백그라운드 알림</strong>
        <Switch checked={background.changeAlerts} label="백그라운드 알림" disabled={checking || background.busy || background.preferencesBusy || !notifications.enabled || !background.status} onchange={setBackgroundAlerts} />
      </div>
      <p class="hint">클래스룸에서 새 알림이 생기거나, 과제, 강의, 자료 등 정보가 추가 혹은 수정되었을 때 알림을 보냅니다</p>
      {#if background.status && (!background.status.registered || !background.status.consented)}<p class="hint">백그라운드 일정 확인을 켜면 적용돼요.</p>{/if}
    </div>
    {#if background.error}<p class="error" role="alert">{background.error}</p>{/if}
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
