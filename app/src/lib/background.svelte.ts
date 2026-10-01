import { ApiError, isApp, request } from './api';
import { isCurrentSession, onSessionChange, readUserData, sessionUser, sessionVersion, writeUserData } from './session';
import { mobileNotifications, notificationsAllowed, setRemoteNotifications } from './notify';
import { notificationState } from './notification-state.svelte';
import { settings } from './settings.svelte';
import { seatPrefs } from './seat.svelte';
import { toast } from './ui.svelte';
import { pref, setPref } from './store.svelte';

type Status = { registered: boolean; changeAlerts?: boolean; consented: boolean; available: { web: boolean; fcm: boolean; apns: boolean }; publicKey: string | null; expiresAt: number | null; lastPollAt: number | null; scheduled: number; nextAt: number | null; error: string | null; schoolError: string | null; pollMinutes: number };
export const background = $state({ status: null as Status | null, busy: false, preferencesBusy: false, changeAlerts: true, error: '', choice: null as boolean | null });
let defaultAttempted = false;
const choiceKey = () => `background-check:${encodeURIComponent(sessionUser() ?? '')}`;
function rememberChoice(enabled: boolean) {
  background.choice = enabled;
  try { setPref(choiceKey(), enabled); } catch {   }
  writeUserData('background-consent', enabled);
}
const alertsKey = () => `background-alerts:${encodeURIComponent(sessionUser() ?? '')}`;
function rememberAlerts(enabled: boolean) {
  background.changeAlerts = enabled;
  try { setPref(alertsKey(), enabled); } catch { }
}
let device = '';
let prefSignature = '';
let preferenceQueue: Promise<void> = Promise.resolve();
let renewedAt = 0;
let connectionVersion = 0;
let renewal: Promise<void> | null = null;
export const pushPlatform = () => !isApp ? 'web' : /Android/i.test(navigator.userAgent) ? 'fcm' : mobileNotifications ? 'apns' : null;
export const pushSupported = () => isApp ? mobileNotifications : 'serviceWorker' in navigator && 'PushManager' in window && window.isSecureContext;
function deviceId() { if (!device) { device = readUserData('push-device', '') || crypto.randomUUID(); writeUserData('push-device', device); } return device; }
const prefs = () => ({ deviceId: deviceId(), leads: [...settings.alertLeads], seatLeads: [...seatPrefs.alerts], changeAlerts: background.changeAlerts });
const problem = (e: unknown) => e instanceof Error ? e.message : '백그라운드 알림 서버에 연결하지 못했어요.';
async function destination(publicKey: string | null) {
  if (isApp) {
    const n = await import('@choochmeque/tauri-plugin-notifications-api');
    return { token: await n.registerForPushNotifications() };
  }
  if (!publicKey) throw new Error('서버 오류입니다. 문제가 지속되면 문의해 주세요.');
  const registration = await navigator.serviceWorker.register('/notification-sw.js', { scope: '/' });
  await navigator.serviceWorker.ready;
  const decoded = atob(publicKey.replace(/-/g, '+').replace(/_/g, '/'));
  const key = Uint8Array.from(decoded, (c) => c.charCodeAt(0));
  const subscription = await registration.pushManager.getSubscription() ?? await registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
  return subscription.toJSON();
}
export async function refreshBackground(renewToken = false) {
  const version = sessionVersion(), connection = connectionVersion;
  const current = () => isCurrentSession(version) && connection === connectionVersion;
  try {
    const status = await request<Status>('GET', `/api/push/status?device=${encodeURIComponent(deviceId())}`);
    if (!current()) return;
    if (!status || typeof status.registered !== 'boolean' || typeof status.consented !== 'boolean'
      || !status.available || !['web', 'fcm', 'apns'].every((key) => typeof status.available[key as keyof Status['available']] === 'boolean')
      || !Number.isFinite(status.scheduled) || status.scheduled < 0) {
      throw new ApiError(200, 'invalid_response', '알림 서버의 응답을 확인하지 못했어요. 잠시 뒤 다시 시도해 주세요.');
    }
    background.status = status;
    background.error = '';
    if (status.registered && !background.preferencesBusy && typeof status.changeAlerts === 'boolean') {
      if (background.changeAlerts !== status.changeAlerts) prefSignature = '';
      rememberAlerts(status.changeAlerts);
    }
    if (background.choice === null && status.registered && status.consented) rememberChoice(true);
    if (status.registered && status.consented && notificationState.enabled && (renewToken || Date.now() - renewedAt > 60 * 60_000)) {
      if (!renewal) {
        const attempt = request<{ expiresAt: number }>('POST', '/api/push/renew', { deviceId: deviceId() }).then((result) => {
          if (!current()) return;
          renewedAt = Date.now();
          status.expiresAt = result.expiresAt;
          status.schoolError = null;
          background.status = status;
        });
        renewal = attempt;
        void attempt.finally(() => { if (renewal === attempt) renewal = null; }).catch(() => {});
      }
      try { await renewal; }
      catch (e) {
        if (!current()) return;
        background.error = e instanceof ApiError && e.status === 401
          ? '학교 로그인이 만료되어 보관 기간을 연장하지 못했어요. 다시 로그인해 주세요.'
          : `학교 세션을 갱신하지 못했어요. ${problem(e)}`;
      }
      if (!current()) return;
    }
    await setRemoteNotifications(status.registered && status.consented && notificationState.enabled);
    if (!current()) return;
    if (renewToken && status.registered && status.consented && notificationState.enabled && await notificationsAllowed()) {
      const dest = await destination(status.publicKey);
      if (!current()) return;
      await savePreferences(undefined, { kind: pushPlatform(), destination: dest });
    }
  } catch (e) {
    if (current()) {
      background.error = problem(e);
    }
  }
}
export async function enableBackground(automatic = false) {
  if (background.busy || background.preferencesBusy) return false;
  background.busy = true; background.error = '';
  connectionVersion++; renewal = null; renewedAt = 0;
  const version = sessionVersion();
  try {
    if (!notificationState.enabled) throw new Error('먼저 이 기기 알림을 켜 주세요.');
    if (!pushSupported()) throw new Error('알림을 지원하지 않는 환경이에요');
    if (!await notificationsAllowed(!automatic)) throw new Error('기기의 알림 권한을 허용해 주세요.');
    if (!isCurrentSession(version)) return false;
    await refreshBackground();
    if (!isCurrentSession(version)) return false;
    if (background.error) throw new Error(background.error);
    const kind = pushPlatform();
    if (!kind || !background.status?.available[kind]) throw new Error('서버 오류입니다. 문제가 지속되면 문의해 주세요.');
    const dest = await destination(background.status.publicKey);
    if (!isCurrentSession(version)) return false;
    const result = await request<{ expiresAt: number }>('POST', '/api/push/device', { ...prefs(), kind, destination: dest, consent: true, onlyThisDevice: false });
    if (!isCurrentSession(version)) return false;
    connectionVersion++;
    rememberChoice(true);
    if (background.status) background.status = { ...background.status, registered: true, consented: true, expiresAt: result.expiresAt };
    await refreshBackground();
    if (!isCurrentSession(version)) return false;
    if (background.error) throw new Error(background.error);
    if (!automatic) toast('백그라운드 일정 확인을 켰어요.', 'success');
    return true;
  } catch (e) { if (isCurrentSession(version)) background.error = problem(e); return false; }
  finally { if (isCurrentSession(version)) background.busy = false; }
}
export async function disableBackground(all = false) {
  if (background.busy || background.preferencesBusy) return false;
  background.busy = true; background.error = '';
  connectionVersion++; renewal = null; renewedAt = 0;
  const version = sessionVersion();
  try {
    await request('DELETE', `/api/push/device?device=${all ? 'all' : encodeURIComponent(deviceId())}`);
    if (!isCurrentSession(version)) return false;
    connectionVersion++;
    rememberChoice(false);
    if (background.status) background.status = { ...background.status, registered: false, consented: all ? false : background.status.consented, scheduled: 0, nextAt: null };
    await setRemoteNotifications(false);
    await refreshBackground();
    return true;
  } catch (e) { if (isCurrentSession(version)) background.error = problem(e); return false; }
  finally { if (isCurrentSession(version)) background.busy = false; }
}
export async function enableBackgroundByDefault(remembered: boolean) {
  const platform = pushPlatform();
  if (!sessionUser() || !remembered || defaultAttempted || background.choice !== null || background.busy
    || background.error || !notificationState.enabled || notificationState.permission !== 'granted' || !pushSupported()
    || !platform || !background.status?.available[platform]) return false;
  if (background.status.registered && background.status.consented) return false;
  defaultAttempted = true;
  return enableBackground(true);
}

export async function setBackgroundEnabled(enabled: boolean) {
  const ok = enabled ? await enableBackground() : await disableBackground(true);
  if (!ok && background.error) toast(background.error, 'error');
}
async function savePreferences(changeAlerts?: boolean, destinationUpdate?: { kind: string | null; destination: unknown }): Promise<boolean> {
  const version = sessionVersion(), connection = connectionVersion;
  const current = () => isCurrentSession(version) && connection === connectionVersion;
  let saved = false;
  const operation = preferenceQueue.then(async () => {
    if (!current() || !background.status?.registered) return;
    const values = { ...prefs(), ...(changeAlerts === undefined ? {} : { changeAlerts }) };
    const signature = JSON.stringify(values);
    if (signature === prefSignature && !destinationUpdate) { saved = true; return; }
    try {
      await request('PUT', '/api/push/preferences', { ...values, ...destinationUpdate });
      if (!current()) return;
      prefSignature = signature;
      if (changeAlerts !== undefined) {
        rememberAlerts(changeAlerts);
        if (background.status) background.status.changeAlerts = changeAlerts;
      }
      background.error = '';
      saved = true;
    } catch (e) { if (current()) background.error = problem(e); }
  });
  preferenceQueue = operation.catch(() => {});
  await operation;
  return saved;
}
export async function syncBackgroundPreferences() {
  await savePreferences();
}
export async function setBackgroundAlerts(enabled: boolean) {
  if (background.busy || background.preferencesBusy) return;
  if (!background.status) {
    toast(background.error || '백그라운드 일정 확인 상태를 먼저 확인해 주세요.', 'error');
    return;
  }
  if (!background.status.registered) { rememberAlerts(enabled); return; }
  const version = sessionVersion();
  background.preferencesBusy = true;
  connectionVersion++;
  try {
    if (!await savePreferences(enabled) && isCurrentSession(version) && background.error) toast(background.error, 'error');
  } finally {
    if (isCurrentSession(version)) {
      connectionVersion++;
      background.preferencesBusy = false;
      if (!background.error) void syncBackgroundPreferences();
    }
  }
}
onSessionChange(() => {
  device = ''; prefSignature = ''; connectionVersion++; renewedAt = 0; renewal = null; defaultAttempted = false;
  background.status = null; background.busy = false; background.preferencesBusy = false; background.error = '';
  background.changeAlerts = pref<boolean>(alertsKey(), true);
  background.choice = sessionUser() ? pref<boolean | null>(choiceKey(), readUserData<boolean | null>('background-consent', null)) : null;
});
