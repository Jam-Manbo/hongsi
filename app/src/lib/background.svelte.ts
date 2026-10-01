import { ApiError, isApp, request } from './api';
import { isCurrentSession, onSessionChange, readUserData, sessionUser, sessionVersion, writeUserData } from './session';
import { cancelNotificationPermission, mobileNotifications, notificationsAllowed, requestNotificationPermission, setNotificationsEnabled, setRemoteNotifications } from './notify';
import { notificationState } from './notification-state.svelte';
import { settings } from './settings.svelte';
import { seatPrefs } from './seat.svelte';
import { toast } from './ui.svelte';
import { pref, setPref } from './store.svelte';

type Status = { registered: boolean; changeAlerts?: boolean; consented: boolean; available: { web: boolean; fcm: boolean; apns: boolean }; publicKey: string | null; expiresAt: number | null; lastPollAt: number | null; scheduled: number; nextAt: number | null; error: string | null; schoolError: string | null; pollMinutes: number };
type Removal = 'all' | 'device' | null;
export const background = $state({ status: null as Status | null, busy: false, changeAlerts: false, error: '', choice: null as boolean | null });
const userKey = (name: string) => `${name}:${encodeURIComponent(sessionUser() ?? '')}`;
function rememberChoice(enabled: boolean) {
  background.choice = enabled;
  try { setPref(userKey('background-check'), enabled); } catch { }
  writeUserData('background-consent', enabled);
}
function rememberAlerts(enabled: boolean) {
  background.changeAlerts = enabled;
  try { setPref(userKey('background-alerts'), enabled); } catch { }
}
function rememberRemoval(value: Removal) {
  removal = value;
  try { setPref(userKey('background-removal'), value); } catch { }
}
let initialized = false;
let device = '';
let removal: Removal = null;
let prefSignature = '';
let renewedAt = 0;
let revision = 0;
let syncTask: Promise<boolean> | null = null;
let syncRequested = false;
let askRequested = false;
let tokenRequested = false;
let retries = 0;
let retryTimer: ReturnType<typeof setTimeout> | undefined;
export const pushPlatform = () => !isApp ? 'web' : /Android/i.test(navigator.userAgent) ? 'fcm' : mobileNotifications ? 'apns' : null;
export const pushSupported = () => isApp ? mobileNotifications : 'serviceWorker' in navigator && 'PushManager' in window && window.isSecureContext;
function deviceId() { if (!device) { device = readUserData('push-device', '') || crypto.randomUUID(); writeUserData('push-device', device); } return device; }
const prefs = () => ({ deviceId: deviceId(), leads: [...settings.alertLeads], seatLeads: [...seatPrefs.alerts], changeAlerts: background.changeAlerts });
const problem = (e: unknown) => e instanceof Error ? e.message : '백그라운드 알림 서버에 연결하지 못했어요.';
function clearRetry() { clearTimeout(retryTimer); retryTimer = undefined; }
function scheduleRetry() {
  clearRetry();
  if (!sessionUser() || (!background.choice && !removal)) return;
  const delay = [10_000, 30_000, 60_000, 300_000][Math.min(retries++, 3)];
  retryTimer = setTimeout(() => { retryTimer = undefined; void refreshBackground(); }, delay);
}
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
async function readStatus(): Promise<Status> {
  const status = await request<Status>('GET', `/api/push/status?device=${encodeURIComponent(deviceId())}`);
  if (!status || typeof status.registered !== 'boolean' || typeof status.consented !== 'boolean'
    || !status.available || !['web', 'fcm', 'apns'].every((key) => typeof status.available[key as keyof Status['available']] === 'boolean')
    || !Number.isFinite(status.scheduled) || status.scheduled < 0) {
    throw new ApiError(200, 'invalid_response', '알림 서버의 응답을 확인하지 못했어요. 잠시 뒤 다시 시도해 주세요.');
  }
  return status;
}
async function reconcile(ask: boolean, renewToken: boolean, check: () => void) {
  if (removal) {
    const target = removal;
    await request('DELETE', `/api/push/device?device=${target === 'all' ? 'all' : encodeURIComponent(deviceId())}`);
    check();
    rememberRemoval(null);
    prefSignature = ''; renewedAt = 0;
    if (background.status) background.status = { ...background.status, registered: false, consented: target === 'all' ? false : background.status.consented, scheduled: 0, nextAt: null };
  }
  if (!background.choice) {
    await setRemoteNotifications(false);
    check();
    return;
  }
  if (!notificationState.enabled) {
    await setNotificationsEnabled(true);
    check();
  }
  const allowed = await (ask ? requestNotificationPermission() : notificationsAllowed());
  check();
  if (!pushSupported()) throw new Error('알림을 지원하지 않는 환경이에요');
  let status = await readStatus();
  check();
  background.status = status;
  if (!allowed) {
    const values = prefs(), signature = JSON.stringify(values);
    if (status.registered && status.consented && signature !== prefSignature) {
      await request('PUT', '/api/push/preferences', values);
      check();
      prefSignature = signature;
    }
    throw new Error('알림 권한을 허용해 주세요. 설정은 켜진 상태로 유지돼요.');
  }
  const kind = pushPlatform();
  if (!kind || !status.available[kind]) throw new Error('알림 서버에 연결하지 못했어요. 자동으로 다시 시도해요.');
  let registered = status.registered && status.consented;
  if (registered && (renewToken || Date.now() - renewedAt > 60 * 60_000)) {
    try {
      const result = await request<{ expiresAt: number }>('POST', '/api/push/renew', { deviceId: deviceId() });
      check();
      renewedAt = Date.now();
      status = { ...status, expiresAt: result.expiresAt, schoolError: null };
      background.status = status;
    } catch (e) {
      check();
      if (e instanceof ApiError && e.status === 409) registered = false;
      else throw e;
    }
  }
  if (!registered) {
    const dest = await destination(status.publicKey);
    check();
    const values = prefs();
    await request('POST', '/api/push/device', { ...values, kind, destination: dest, consent: true, onlyThisDevice: false });
    check();
    status = await readStatus();
    check();
    background.status = status;
    if (!status.registered || !status.consented) throw new Error('알림 등록을 완료하지 못했어요. 자동으로 다시 시도해요.');
    prefSignature = JSON.stringify(values);
    renewedAt = Date.now();
  } else {
    const values = prefs();
    const signature = JSON.stringify(values);
    const dest = renewToken ? await destination(status.publicKey) : undefined;
    check();
    if (signature !== prefSignature || dest) {
      await request('PUT', '/api/push/preferences', { ...values, ...(dest ? { kind, destination: dest } : {}) });
      check();
      prefSignature = signature;
    }
  }
  background.status = { ...status, changeAlerts: background.changeAlerts };
  await setRemoteNotifications(true);
  check();
}
function synchronize(ask = false, renewToken = false): Promise<boolean> {
  if (!sessionUser() || background.choice === null) return Promise.resolve(false);
  clearRetry();
  syncRequested = true;
  askRequested ||= ask;
  tokenRequested ||= renewToken;
  if (syncTask) return syncTask;
  const version = sessionVersion();
  background.busy = true;
  const task = (async () => {
    let ok = false;
    while (syncRequested && isCurrentSession(version)) {
      syncRequested = false;
      const attempt = revision;
      const ask = askRequested, renewToken = tokenRequested;
      askRequested = false; tokenRequested = false;
      const current = () => isCurrentSession(version) && revision === attempt;
      const check = () => { if (!current()) throw new Error('설정이 변경됐어요.'); };
      try {
        await reconcile(ask, renewToken, check);
        check();
        background.error = '';
        retries = 0;
        ok = true;
      } catch (e) {
        ok = false;
        if (current()) background.error = background.choice ? problem(e) : `해제를 완료하지 못했어요. 다시 시도해요. ${problem(e)}`;
      }
    }
    return ok;
  })().finally(() => {
    if (syncTask !== task) return;
    syncTask = null;
    if (!isCurrentSession(version)) return;
    background.busy = false;
    if (background.error) scheduleRetry();
  });
  syncTask = task;
  return task;
}
export const refreshBackground = (renewToken = false) => synchronize(false, renewToken);
export async function enableBackground() {
  revision++; retries = 0;
  rememberChoice(true);
  background.error = '';
  return synchronize(true);
}
export async function disableBackground(all = false) {
  revision++; retries = 0;
  rememberChoice(false);
  rememberRemoval(all || removal === 'all' ? 'all' : 'device');
  background.error = '';
  clearRetry();
  askRequested = false; tokenRequested = false;
  cancelNotificationPermission();
  void setRemoteNotifications(false);
  return synchronize();
}
export async function enableBackgroundByDefault(remembered: boolean) {
  if (!sessionUser() || initialized) return false;
  initialized = true;
  if (background.choice === null) {
    if (remembered) {
      rememberChoice(true);
      rememberAlerts(pref<boolean>(userKey('background-alerts'), true));
    } else {
      background.choice = false;
    }
  }
  return synchronize(background.choice === true, true);
}
export async function setBackgroundEnabled(enabled: boolean) {
  await (enabled ? enableBackground() : disableBackground(true));
}
export const syncBackgroundPreferences = () => synchronize();
export async function setBackgroundAlerts(enabled: boolean) {
  rememberAlerts(enabled);
  if (background.choice) await synchronize(enabled);
}
export async function retryBackgroundPermission() {
  const version = sessionVersion();
  const allowed = await requestNotificationPermission();
  if (!isCurrentSession(version)) return;
  if (allowed) await refreshBackground();
  else toast('권한창이 다시 열리지 않으면 브라우저·기기 설정에서 홍시 알림을 허용해 주세요.', 'info');
}
onSessionChange(() => {
  clearRetry(); cancelNotificationPermission();
  initialized = false; device = ''; prefSignature = ''; revision++; renewedAt = 0; retries = 0;
  syncTask = null; syncRequested = false; askRequested = false; tokenRequested = false;
  background.status = null; background.busy = false; background.error = '';
  background.choice = sessionUser() ? pref<boolean | null>(userKey('background-check'), readUserData<boolean | null>('background-consent', null)) : null;
  background.changeAlerts = sessionUser() ? pref<boolean>(userKey('background-alerts'), background.choice === true) : false;
  removal = sessionUser() ? pref<Removal>(userKey('background-removal'), null) : null;
});
if (typeof window !== 'undefined') {
  const resume = () => { if (initialized && document.visibilityState === 'visible') void refreshBackground(); };
  window.addEventListener('online', resume);
  window.addEventListener('focus', resume);
  document.addEventListener('visibilitychange', resume);
}
