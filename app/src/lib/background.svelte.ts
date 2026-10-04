import { ApiError, isApp, request } from './api';
import { contextMessage } from './api-error';
import { mobileNotifications, notificationsAllowed, requestNotificationPermission, setRemoteNotifications } from './notify';
import { isCurrentSession, onSessionChange, readUserData, sessionUser, sessionVersion, writeUserData } from './session';
import { seatPrefs } from './seat.svelte';
import { app, onBeforeLogout, pref, setPref } from './store.svelte';

type Status = { registered: boolean; classroomAlerts: boolean; consented: boolean; available: { web: boolean; fcm: boolean; apns: boolean }; publicKey: string | null; expiresAt: number | null; lastPollAt: number | null; scheduled: number; nextAt: number | null; error: string | null; schoolError: string | null; pollMinutes: number };
type Removal = 'all' | 'device' | null;
export const background = $state({ status: null as Status | null, busy: false, classroomAlerts: false, error: '', choice: null as boolean | null });
const userKey = (name: string) => `${name}:${encodeURIComponent(sessionUser() ?? '')}`;
let initialized = false;
let device = '';
let classroomEpoch = '';
let removal: Removal = null;
let prefSignature = '';
let renewedAt = 0;
let revision = 0;
let syncTask: Promise<boolean> | null = null;
let syncRequested = false;
let tokenRequested = false;
let retries = 0;
let retryTimer: ReturnType<typeof setTimeout> | undefined;

function rememberChoice(enabled: boolean) {
  background.choice = enabled;
  setPref(userKey('sync-enabled-v2'), enabled);
}
function rememberAlerts(enabled: boolean) {
  background.classroomAlerts = enabled;
  setPref(userKey('classroom-alerts-v2'), enabled);
  if (enabled || !classroomEpoch) {
    classroomEpoch = crypto.randomUUID();
    setPref(userKey('classroom-cycle-v2'), classroomEpoch);
  }
}
function rememberRemoval(value: Removal) {
  removal = value;
  setPref(userKey('sync-removal-v2'), value);
}
function deviceId() {
  if (!device) {
    device = readUserData('sync-device-v2', '') || crypto.randomUUID();
    writeUserData('sync-device-v2', device);
  }
  return device;
}
function prefs() {
  if (!classroomEpoch) {
    classroomEpoch = crypto.randomUUID();
    setPref(userKey('classroom-cycle-v2'), classroomEpoch);
  }
  return { deviceId: deviceId(), seatLeads: [...seatPrefs.alerts], classroomAlerts: background.classroomAlerts, classroomEpoch };
}
const pushPlatform = () => !isApp ? 'web' : /Android/i.test(navigator.userAgent) ? 'fcm' : mobileNotifications ? 'apns' : null;
const pushSupported = () => isApp ? mobileNotifications : 'serviceWorker' in navigator && 'PushManager' in window && window.isSecureContext;
class SyncError extends Error {}
type SyncStep = 'status' | 'connect' | 'settings' | 'disconnect';
const failures: Record<SyncStep, string> = {
  status: '알림 연결 상태를 확인하지 못했어요.',
  connect: '백그라운드 동기화에 연결하지 못했어요.',
  settings: '알림 설정을 적용하지 못했어요.',
  disconnect: '백그라운드 동기화를 끄지 못했어요. 자동으로 다시 시도할게요.',
};
const problem = (e: unknown, step: SyncStep) => e instanceof SyncError ? e.message : contextMessage(e, failures[step]);
function clearRetry() { clearTimeout(retryTimer); retryTimer = undefined; }
function scheduleRetry() {
  clearRetry();
  if (!sessionUser() || app.loggingOut || (!background.choice && !removal)) return;
  const delay = [10_000, 30_000, 60_000, 300_000][Math.min(retries++, 3)];
  retryTimer = setTimeout(() => { retryTimer = undefined; void refreshBackground(); }, delay);
}
async function destination(publicKey: string | null) {
  if (isApp) {
    const n = await import('@choochmeque/tauri-plugin-notifications-api');
    return { token: await n.registerForPushNotifications() };
  }
  if (!publicKey) throw new SyncError('알림 서버의 설정을 확인하지 못했어요. 다시 시도해 주세요.');
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
    || typeof status.classroomAlerts !== 'boolean' || !status.available
    || !['web', 'fcm', 'apns'].every((key) => typeof status.available[key as keyof Status['available']] === 'boolean')
    || !Number.isFinite(status.scheduled) || status.scheduled < 0) {
    throw new ApiError(200, 'invalid_response', '동기화 서버의 응답을 확인하지 못했어요. 잠시 뒤 다시 시도해 주세요.');
  }
  return status;
}
async function reconcile(renewToken: boolean, check: () => void, step: (value: SyncStep) => void) {
  if (removal) {
    step('disconnect');
    const target = removal;
    await request('DELETE', `/api/background/session?device=${target === 'all' ? 'all' : encodeURIComponent(deviceId())}`);
    check();
    rememberRemoval(null);
    prefSignature = ''; renewedAt = 0;
    background.status = null;
  }
  if (!background.choice) {
    await setRemoteNotifications(false);
    check();
    return;
  }
  step('status');
  let status = await readStatus();
  check();
  background.status = status;
  step('connect');
  let consented = status.consented;
  if (consented && (renewToken || Date.now() - renewedAt > 60 * 60_000)) {
    try {
      const result = await request<{ expiresAt: number }>('POST', '/api/background/renew', { deviceId: deviceId() });
      check();
      renewedAt = Date.now();
      status = { ...status, expiresAt: result.expiresAt, schoolError: null };
    } catch (e) {
      check();
      if (e instanceof ApiError && e.status === 409) consented = false;
      else throw e;
    }
  }
  if (!consented) {
    await request('POST', '/api/background/session', { deviceId: deviceId(), consent: true });
    check();
    status = await readStatus();
    check();
    if (!status.consented) throw new SyncError('백그라운드 동기화에 연결하지 못했어요.');
    renewedAt = Date.now();
    prefSignature = '';
  }
  background.status = status;
  step('settings');
  const allowed = await notificationsAllowed();
  check();
  const kind = pushPlatform();
  if (!allowed || !pushSupported() || !kind) {
    if (status.registered) {
      await request('DELETE', `/api/push/device?device=${encodeURIComponent(deviceId())}`);
      check();
    }
    prefSignature = '';
    background.status = { ...status, registered: false, scheduled: 0, nextAt: null, error: null };
    await setRemoteNotifications(false);
    check();
    return;
  }
  if (!status.available[kind]) {
    await setRemoteNotifications(false);
    check();
    throw new SyncError('서버의 알림 발송 설정을 확인하지 못했어요.');
  }
  const values = prefs(), signature = JSON.stringify(values);
  if (!status.registered || renewToken) {
    const dest = await destination(status.publicKey);
    check();
    await request('POST', '/api/push/device', { ...values, kind, destination: dest });
    check();
    status = await readStatus();
    check();
    if (!status.registered) throw new SyncError('알림 수신 설정을 완료하지 못했어요. 자동으로 다시 시도할게요.');
  } else if (signature !== prefSignature || status.classroomAlerts !== values.classroomAlerts) {
    await request('PUT', '/api/push/preferences', values);
    check();
  }
  prefSignature = signature;
  background.status = { ...status, classroomAlerts: values.classroomAlerts };
  await setRemoteNotifications(true);
  check();
}
function synchronize(renewToken = false): Promise<boolean> {
  if (!sessionUser() || app.loggingOut || background.choice === null) return Promise.resolve(false);
  clearRetry();
  syncRequested = true;
  tokenRequested ||= renewToken;
  if (syncTask) return syncTask;
  const version = sessionVersion();
  background.busy = true;
  const task = (async () => {
    let ok = false;
    while (syncRequested && isCurrentSession(version) && !app.loggingOut) {
      syncRequested = false;
      const attempt = revision, renew = tokenRequested;
      tokenRequested = false;
      const current = () => isCurrentSession(version) && revision === attempt && !app.loggingOut;
      const check = () => { if (!current()) throw new Error('설정이 변경됐어요.'); };
      let step: SyncStep = 'connect';
      try {
        await reconcile(renew, check, (value) => { step = value; });
        check();
        background.error = '';
        retries = 0;
        ok = true;
      } catch (e) {
        ok = false;
        if (current()) background.error = problem(e, background.choice ? step : 'disconnect');
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
export const refreshBackground = (renewToken = false) => synchronize(renewToken);
async function pauseBackgroundForLogout() {
  clearRetry();
  revision++;
  syncRequested = false;
  tokenRequested = false;
  await syncTask;
}
onBeforeLogout(pauseBackgroundForLogout);
export async function setBackgroundEnabled(enabled: boolean) {
  if (app.loggingOut) return false;
  revision++; retries = 0;
  rememberChoice(enabled);
  background.error = '';
  if (!enabled) {
    rememberRemoval('all');
    clearRetry();
    void setRemoteNotifications(false);
  }
  return synchronize();
}
export async function enableBackgroundByDefault(remembered: boolean) {
  if (!sessionUser() || app.loggingOut || initialized) return false;
  initialized = true;
  if (background.choice === null) {
    if (remembered) rememberChoice(true);
    else background.choice = false;
  }
  return synchronize(true);
}
export const syncBackgroundPreferences = () => synchronize();
export async function setClassroomAlerts(enabled: boolean) {
  if (app.loggingOut || !background.choice || background.classroomAlerts === enabled) return;
  revision++;
  rememberAlerts(enabled);
  const version = sessionVersion();
  void synchronize();
  if (enabled) {
    await requestNotificationPermission();
    if (isCurrentSession(version)) await synchronize();
  }
}
export async function configureNotificationPermission() {
  if (app.loggingOut) return;
  const version = sessionVersion();
  await requestNotificationPermission();
  if (isCurrentSession(version)) await refreshBackground(true);
}
onSessionChange(() => {
  clearRetry();
  initialized = false; device = ''; prefSignature = ''; revision++; renewedAt = 0; retries = 0;
  syncTask = null; syncRequested = false; tokenRequested = false;
  background.status = null; background.busy = false; background.error = '';
  background.choice = sessionUser() ? pref<boolean | null>(userKey('sync-enabled-v2'), null) : null;
  background.classroomAlerts = sessionUser() ? pref<boolean>(userKey('classroom-alerts-v2'), false) : false;
  classroomEpoch = sessionUser() ? pref<string>(userKey('classroom-cycle-v2'), '') : '';
  removal = sessionUser() ? pref<Removal>(userKey('sync-removal-v2'), null) : null;
});
if (typeof window !== 'undefined') {
  const resume = () => { if (initialized && document.visibilityState === 'visible') void refreshBackground(); };
  window.addEventListener('online', resume);
  window.addEventListener('focus', resume);
  document.addEventListener('visibilitychange', resume);
}
