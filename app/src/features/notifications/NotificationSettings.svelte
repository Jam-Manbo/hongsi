<script lang="ts">
  import { sentenceLines } from '../../shared/utils/format';
  import { onMount } from 'svelte';
  import { ALERT_LEADS, settings, toggleAlertLead, accountPreferences } from '../settings/settings.svelte';
  import { notificationsAllowed, notificationState as notifications } from './notify';
  import { background, setBackgroundEnabled, setClassroomAlerts, refreshBackground, configureNotificationPermission } from './background.svelte';
  import { app } from '../auth/auth-state.svelte';
  import Switch from '../../shared/ui/Switch.svelte';
  import Icon from '../../shared/ui/Icon.svelte';

  const permissionText = { unknown: '권한을 확인하고 있어요.', granted: '알림이 허용되어 있어요.', denied: '기기·브라우저 설정에서 알림을 허용해 주세요.', default: '알림을 받으려면 권한을 허용해 주세요.', unsupported: '알림을 지원하지 않는 환경이에요.' };
  const local = $derived([notifications.due, notifications.seat]);
  let checking = $state(false);
  async function checkBackground() {
    if (checking) return;
    checking = true;
    try { await refreshBackground(); } finally { checking = false; }
  }
  onMount(() => { void notificationsAllowed(); void checkBackground(); });
  const errors = $derived([...new Set([notifications.error, ...local.map((c) => c.error), background.error || background.status?.schoolError || background.status?.error].filter((error): error is string => !!error))]);
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
    {#each ALERT_LEADS as l (l.min)}<button class="filter" aria-pressed={settings.alertLeads.includes(l.min)} disabled={!accountPreferences.loaded} onclick={() => toggleAlertLead(l.min)}>{l.label}</button>{/each}
  </div>
  <p class="hint">과제, 강의와 할 일에 기본으로 적용해요.</p>
  <div class="background">
    <div class="setting-head">
      <strong>백그라운드 동기화</strong>
      <Switch checked={background.choice === true} label="백그라운드 동기화" disabled={app.loggingOut} onchange={setBackgroundEnabled} />
    </div>
    <p class="hint">일정 및 클래스룸 알림을 동기화해요.</p>
    <div class="classroom-alerts">
      <div class="setting-head">
        <strong>클래스룸 알림</strong>
        <Switch checked={background.classroomAlerts} label="클래스룸 알림" disabled={app.loggingOut || background.choice !== true} onchange={setClassroomAlerts} />
      </div>
      <p class="hint">클래스룸에 새 알림이 생기면 알려줘요.</p>
    </div>
  </div>
  {#if errors.length}
    <div class="background-status">
      <div>{#each errors as error}<p class="error sentence-message" role="alert">{sentenceLines(error)}</p>{/each}</div>
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
