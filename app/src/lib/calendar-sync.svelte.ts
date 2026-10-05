import { api } from './api';
import { schoolFinished } from './colors';
import { app, calendar, handleAuthError, todos } from './store.svelte';
import { isCurrentSession, onSessionChange, sessionVersion } from './session';

export const calendarSync = $state({ updatedAt: 0 });
let revision = 0;
let writing = 0;
let lastRead = 0;
let reading: Promise<void> | null = null;

export function beginCalendarChange() {
  const version = sessionVersion();
  writing += 1;
  revision += 1;
  return () => {
    if (!isCurrentSession(version)) return;
    writing -= 1;
    revision += 1;
  };
}

async function refreshCalendarState() {
  if (reading) return reading;
  if (!app.profile || app.loggingOut || !calendar.data || calendar.loading || writing || Date.now() - lastRead < 3_000) return;
  lastRead = Date.now();
  const version = sessionVersion(), started = revision, before = calendar.data;
  const job = (async () => {
    try {
      const state = await api.calendarState();
      if (!isCurrentSession(version) || revision !== started || writing || calendar.loading || calendar.data !== before) return;
      const off = new Set(state.alertsOff);
      let changed = false;
      const items = before.items.map((item) => {
        const doneOverride = state.checks[item.key] ?? null;
        const done = doneOverride ?? schoolFinished(item);
        const alert = !off.has(item.key);
        const alertLeads = state.alertLeads[item.key] ?? null;
        if (item.done === done && item.doneOverride === doneOverride && item.alert === alert
          && JSON.stringify(item.alertLeads) === JSON.stringify(alertLeads)) return item;
        changed = true;
        return { ...item, done, doneOverride, alert, alertLeads };
      });
      if (changed) {
        calendar.set({ ...before, items }, calendar.at);
        calendarSync.updatedAt = Date.now();
      }
      await todos.refresh();
    } catch (e) {
      if (isCurrentSession(version)) handleAuthError(e);
    }
  })().finally(() => { if (reading === job) reading = null; });
  reading = job;
  return job;
}

export function watchCalendarState() {
  const refresh = () => { if (document.visibilityState === 'visible') void refreshCalendarState(); };
  refresh();
  window.addEventListener('focus', refresh);
  window.addEventListener('online', refresh);
  document.addEventListener('visibilitychange', refresh);
  const timer = setInterval(refresh, 30_000);
  return () => {
    clearInterval(timer);
    window.removeEventListener('focus', refresh);
    window.removeEventListener('online', refresh);
    document.removeEventListener('visibilitychange', refresh);
  };
}

onSessionChange(() => {
  revision += 1;
  writing = 0;
  reading = null;
  lastRead = 0;
  calendarSync.updatedAt = 0;
});
