import { isApp } from './api';
import { toast } from './ui.svelte';
import { isCurrentSession, onSessionChange, readUserData, sessionUser, sessionVersion, writeUserData } from './session';
import { ReminderScheduler, type NotificationDriver } from './notification-engine';
import { parseIntent, type NotificationIntent, type Permission } from './notification-model';
import { notificationPermission, notificationState as state } from './notification-state.svelte';
export { notificationPermission } from './notification-state.svelte';
export type { Reminder, Channel } from './notification-model';
export { notificationState } from './notification-state.svelte';
export const mobileNotifications = isApp && (/Android|iPhone|iPad/i.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1));
const ios = /iPhone|iPad/i.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
const plugin = () => import('@choochmeque/tauri-plugin-notifications-api');
const PENDING = 'hc:notification-intent';

export function queueNotificationIntent(raw: unknown) {
  const intent = parseIntent(raw);
  if (!intent) return;
  state.pending = intent;
  try { sessionStorage.setItem(PENDING, JSON.stringify(intent)); } catch {   }
}
export function takeNotificationIntent() {
  const intent = state.pending;
  state.pending = null;
  try { sessionStorage.removeItem(PENDING); } catch {   }
  return intent;
}
export function notificationToast(title: string, body: string, raw: unknown) {
  const intent = parseIntent(raw);
  if (!intent || intent.account !== sessionUser()) return;
  toast(`${title} · ${body}`, 'alarm', 8000, () => queueNotificationIntent(intent));
}

const driver: NotificationDriver = {
  native: mobileNotifications, ios,
  async permission(ask): Promise<Permission> {
    if (isApp) {
      const n = await plugin();
      if (await n.isPermissionGranted()) return 'granted';
      if (!ask) return 'denied';
      const p = await n.requestPermission();
      return p === 'granted' ? 'granted' : p === 'denied' ? 'denied' : 'default';
    }
    if (!('Notification' in window)) return 'unsupported';
    const p = ask && Notification.permission === 'default' ? await Notification.requestPermission() : Notification.permission;
    return p;
  },
  async pending() { return (await (await plugin()).pending()).map((p) => p.id); },
  async cancel(ids) { await (await plugin()).cancel(ids); },
  async send(id, reminder, intent, scheduled) {
    if (intent.account !== sessionUser()) return;
    if (isApp) {
      const n = await plugin();
      if (intent.account !== sessionUser()) return;
      await n.sendNotification({ id, title: reminder.title, body: reminder.body, extra: { intent: JSON.stringify(intent) }, autoCancel: true,
        ...(mobileNotifications && !ios ? { icon: 'ic_notification', iconColor: '#FF7A3D' } : {}),
        ...(scheduled ? { schedule: n.Schedule.at(new Date(reminder.at), false, true) } : {}) });
    } else {
      const notification = new Notification(reminder.title, { body: reminder.body, icon: '/favicon.svg', tag: reminder.key });
      notification.onclick = () => { window.focus(); queueNotificationIntent(intent); notification.close(); };
      notificationToast(reminder.title, reminder.body, intent);
    }
  },
};
const scheduler = new ReminderScheduler(driver, (channel, status) => { state[channel] = status; }, (permission) => { state.permission = permission; });
export const scheduleReminders = scheduler.set.bind(scheduler);
export const clearReminders = (channel: 'seat' | 'due') => scheduler.set(channel, []);
export const notificationsAllowed = (ask = false) => scheduler.allow(ask).catch(() => { state.error = '알림 권한을 확인하지 못했어요.'; return false; });
let permissionPrompt: Promise<boolean> | null = null;
let resolvePermission: ((allowed: boolean) => void) | null = null;
export function cancelNotificationPermission() {
  const resolve = resolvePermission;
  resolvePermission = null;
  permissionPrompt = null;
  notificationPermission.open = false;
  notificationPermission.busy = false;
  resolve?.(false);
}
export function requestNotificationPermission(): Promise<boolean> {
  if (permissionPrompt) return permissionPrompt;
  if (isApp || !('Notification' in window) || Notification.permission === 'granted' || navigator.userActivation?.isActive) return notificationsAllowed(true);
  notificationPermission.open = true;
  permissionPrompt = new Promise((resolve) => { resolvePermission = resolve; });
  return permissionPrompt;
}
export async function acceptNotificationPermission() {
  if (notificationPermission.busy) return;
  const version = sessionVersion(), resolve = resolvePermission;
  notificationPermission.busy = true;
  const allowed = await notificationsAllowed(true);
  if (!isCurrentSession(version) || resolve !== resolvePermission) return;
  resolvePermission = null; permissionPrompt = null;
  notificationPermission.open = false; notificationPermission.busy = false;
  resolve?.(allowed);
}
export const refreshNotifications = () => scheduler.refresh(true);
export async function setNotificationsEnabled(enabled: boolean) {
  state.enabled = scheduler.enabled = enabled;
  writeUserData('notifications-enabled', enabled);
  await scheduler.refresh(true);
}
export async function setRemoteNotifications(remote: boolean) {
  state.remote = scheduler.remote = remote;
  writeUserData('remote-active', remote);
  await scheduler.refresh(true);
}
export async function initNotifications() {
  const cleanups: (() => void)[] = [];
  try {
    try { queueNotificationIntent(JSON.parse(sessionStorage.getItem(PENDING) ?? 'null')); } catch {   }
    if (isApp) {
      const n = await plugin();
      const clicked = await n.onNotificationClicked((data) => queueNotificationIntent(data.data?.intent));
      cleanups.push(() => { void clicked.unregister(); });
      const received = await n.onNotificationReceived((data) => notificationToast(data.title, data.body ?? '', data.extra?.intent));
      cleanups.push(() => { void received.unregister(); });
    } else if ('serviceWorker' in navigator) {
      const listener = (event: MessageEvent) => { if (event.data?.type === 'notification-click') queueNotificationIntent(event.data.intent); };
      navigator.serviceWorker.addEventListener('message', listener);
      cleanups.push(() => navigator.serviceWorker.removeEventListener('message', listener));
      const raw = new URLSearchParams(location.hash.split('?')[1] ?? '').get('notification');
      if (raw) { queueNotificationIntent(raw); history.replaceState(null, '', location.pathname + '#/home'); }
    }
  } catch { state.error = '알림 연결 중 오류가 발생했어요. 앱을 다시 열어 주세요.'; }
  return () => cleanups.forEach((fn) => fn());
}

onSessionChange(() => {
  cancelNotificationPermission();
  state.enabled = scheduler.enabled = readUserData('notifications-enabled', true);
  state.remote = scheduler.remote = readUserData('remote-active', false);
  state.error = '';
  void scheduler.reset(sessionUser());
});
