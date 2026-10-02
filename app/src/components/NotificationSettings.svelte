<script lang="ts">
  import { onMount } from 'svelte';
  import { ALERT_LEADS, settings, toggleAlertLead } from '../lib/settings.svelte';
  import { notificationsAllowed, notificationState as notifications } from '../lib/notify';
  import { background, setBackgroundEnabled, setClassroomAlerts, refreshBackground, configureNotificationPermission } from '../lib/background.svelte';
  import { app } from '../lib/store.svelte';
  import Switch from './Switch.svelte';
  import Icon from './Icon.svelte';

  const permissionText = { unknown: '권한을 확인하고 있어요.', granted: '알림이 허용되어 있어요.', denied: '기기·브라우저 설정에서 알림을 허용해 주세요.', default: '알림을 받으려면 권한을 허용해 주세요.', unsupported: '알림을 지원하지 않는 환경이에요' };
  const local = $derived([notifications.due, notifications.seat]);
  let checking = $state(false);
  async function checkBackground() {
    if (checking) return;
    checking = true;
    try { await refreshBackground(); } finally { checking = false; }
  }
  onMount(() => { void notificationsAllowed(); void checkBackground(); });
  const errors = $derived([...new Set([notifications.error, ...local.map((c) => c.error), background.error || background.status?.schoolError || background.status?.error].filter(Boolean))]);
</script>

<div class="notification-settings">
  <div class="setting-head">
    <div><strong>기기 알림</strong><p class="hint">{permissionText[notifications.permission]}</p></div>
    {#if notifications.permission !== 'granted' && notifications.permission !== 'unsupported'}
      <button class="filter" disabled={app.loggingOut || notifications.permission === 'unknown'} onclick={configureNotificationPermission}>권한 설정</button>
    {/if}
  </div>
  <strong class="label">마감 알림</strong>
  <div class="leads" role="group" aria-label="마감 알림 시간">
    {#each ALERT_LEADS as l (l.min)}<button class="filter" aria-pressed={settings.alertLeads.includes(l.min)} onclick={() => toggleAlertLead(l.min)}>{l.label}</button>{/each}
  </div>
  <p class="hint">과제·강의·할 일에 기본으로 적용해요. 각 항목에서 따로 바꿀 수 있어요.</p>
  <div class="background">
    <div class="setting-head">
      <strong>백그라운드 동기화</strong>
      <Switch checked={background.choice === true} label="백그라운드 동기화" disabled={app.loggingOut} onchange={setBackgroundEnabled} />
    </div>
    <p class="hint">앱을 닫아도 서버에서 {background.status?.pollMinutes ?? 5}분마다 일정과 클래스룸 알림을 확인합니다. 학번과 학교 로그인 세션을 암호화해 최대 14일 보관하고, 유효한 로그인 상태로 이용하면 보관 기한을 갱신합니다. 학교 세션의 유효기간을 연장하는 것은 아닙니다.</p>
    <p class="hint">끄면 이 계정의 모든 기기에 대한 백그라운드용 세션과 동기화·푸시 등록을 삭제합니다. 각 기기의 로그인과 자동 로그인 설정은 유지됩니다.</p>
    <div class="classroom-alerts">
      <div class="setting-head">
        <strong>클래스룸 알림</strong>
        <Switch checked={background.classroomAlerts} label="클래스룸 알림" disabled={app.loggingOut || background.choice !== true} onchange={setClassroomAlerts} />
      </div>
      <p class="hint">클래스룸에 새 알림이 생기면 알려드립니다.</p>
      {#if background.choice !== true}<p class="hint">백그라운드 동기화를 켜면 사용할 수 있어요.</p>{/if}
    </div>
  </div>
  {#if errors.length}
    <div class="background-status">
      <div>{#each errors as error}<p class="error" role="alert">{error}</p>{/each}</div>
      <button class="icon-btn" aria-label="설정 다시 적용" disabled={app.loggingOut || checking || background.busy} onclick={checkBackground}><Icon name="refresh" size={18} /></button>
    </div>
  {/if}
</div>
<style>
  .notification-settings { display: grid; gap: 12px; }
  .setting-head { display:flex;align-items:center;justify-content:space-between;gap:12px; }
  .setting-head > div { min-width:0; }
  .setting-head > button { flex:none; }
  .setting-head strong,.label {font-size:14px;}
  .hint {font-size:13px;color:var(--text-2);line-height:1.6;}
  .setting-head .hint { margin-top:4px; }
  .leads {display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:6px;}
  .leads .filter {padding:0 4px;}
  .error {font-size:13px;color:var(--danger);line-height:1.6;}
  .background-status {display:flex;align-items:flex-start;gap:8px;}
  .background-status > div {flex:1;min-width:0;}
  .background-status .icon-btn {flex:none;}
  .classroom-alerts {display:grid;gap:8px;margin-top:12px;}
  .background {border-top:1px solid var(--border);padding-top:20px;display:grid;gap:8px;}
</style>
