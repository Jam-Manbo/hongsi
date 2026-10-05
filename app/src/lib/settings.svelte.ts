import { api } from './api';
import { app, handleAuthError, onBeforeLogout, pref, setPref, setCalendarSemesterDisplay } from './store.svelte';
import { isCurrentSession, onSessionChange, readUserData, sessionUser, sessionVersion, writeUserData } from './session';
import { toastOnce } from './ui.svelte';
import type { AccountPreferences, AccountPreferenceChanges, SemesterDisplay } from './types';

export type Theme = 'system' | 'light' | 'dark';
export type Midnight = 'prev' | 'same';
export type MealPlace = 'dorm' | 'staff';
export type TimetableDisplay = 'full' | 'fit';

export const ALERT_LEADS: { min: number; label: string }[] = [
  { min: 1440, label: '1일 전' },
  { min: 180, label: '3시간 전' },
  { min: 60, label: '1시간 전' },
  { min: 10, label: '10분 전' },
  { min: 0, label: '마감' },
];

export const settings = $state({
  theme: pref<Theme>('theme', 'system'),
  midnight: pref<Midnight>('midnight', 'prev'),
  showUndatedAssignments: pref<boolean>('show-undated-assignments', false),
  mealPlace: 'dorm' as MealPlace,
  timetableDisplay: 'fit' as TimetableDisplay,
  semesterDisplay: 'current' as SemesterDisplay,
  alertLeads: [60],
});

export function toggleAlertLead(min: number) {
  const next = settings.alertLeads.includes(min)
    ? settings.alertLeads.filter((m) => m !== min)
    : [...settings.alertLeads, min];
  changeAccountPreferences({ alertLeads: next.sort((a, b) => b - a) });
}

export function setTimetableDisplay(value: TimetableDisplay) {
  changeAccountPreferences({ timetableDisplay: value });
}

export function setSemesterDisplay(value: SemesterDisplay) {
  changeAccountPreferences({ semesterDisplay: value });
}

export function setMealPlace(value: MealPlace) {
  changeAccountPreferences({ mealPlace: value });
}

export function isPreferredPlace(name: string, place: MealPlace = settings.mealPlace): boolean {
  return place === 'dorm' ? name.includes('기숙사') : name.includes('교직원');
}

export function applyTheme(theme: Theme = settings.theme) {
  const root = document.documentElement;
  if (theme === 'system') delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export function setTheme(theme: Theme) {
  settings.theme = theme;
  setPref('theme', theme);
  applyTheme(theme);
}

export function setMidnight(value: Midnight) {
  settings.midnight = value;
  setPref('midnight', value);
}

export function setShowUndatedAssignments(value: boolean) {
  settings.showUndatedAssignments = value;
  setPref('show-undated-assignments', value);
}

const defaults = (): AccountPreferences => ({ mealPlace: 'dorm', timetableDisplay: 'fit', semesterDisplay: 'current', alertLeads: [60], updatedAt: 0 });
export const accountPreferences = $state({ loaded: false, saving: false, error: '', updatedAt: 0 });
let confirmed = defaults();
let pending: AccountPreferenceChanges = {};
let saving: Promise<void> | null = null;
let reading: Promise<void> | null = null;
let revision = 0;
let lastRead = 0;

function apply(value: AccountPreferences, changes: AccountPreferenceChanges = {}) {
  const next = { ...value, ...changes };
  settings.mealPlace = next.mealPlace;
  settings.timetableDisplay = next.timetableDisplay;
  settings.semesterDisplay = next.semesterDisplay;
  setCalendarSemesterDisplay(settings.semesterDisplay);
  settings.alertLeads = [...next.alertLeads];
  accountPreferences.updatedAt = value.updatedAt;
}

function accept(value: AccountPreferences) {
  confirmed = value;
  accountPreferences.loaded = true;
  accountPreferences.error = '';
  writeUserData('account-preferences', value);
  apply(value, pending);
}

function changeAccountPreferences(changes: AccountPreferenceChanges) {
  if (!sessionUser() || app.loggingOut || !accountPreferences.loaded) return;
  revision++;
  pending = { ...pending, ...changes };
  Object.assign(settings, changes);
  if (changes.semesterDisplay) setCalendarSemesterDisplay(changes.semesterDisplay);
  if (saving) return;
  const version = sessionVersion();
  accountPreferences.saving = true;
  const job = (async () => {
    while (isCurrentSession(version) && Object.keys(pending).length) {
      const changes = pending;
      pending = {};
      try {
        const result = await api.updatePreferences(changes);
        if (!isCurrentSession(version)) return;
        accept(result);
      } catch (e) {
        if (!isCurrentSession(version)) return;
        if (handleAuthError(e)) return;
        apply(confirmed, pending);
        accountPreferences.error = '설정을 저장하지 못했어요.';
        toastOnce(accountPreferences.error, 'error');
      }
    }
  })().finally(() => {
    if (saving !== job) return;
    saving = null;
    accountPreferences.saving = false;
  });
  saving = job;
}

export function refreshAccountPreferences(force = false): Promise<void> {
  if (!sessionUser() || app.loggingOut) return Promise.resolve();
  if (saving) return saving;
  if (reading) {
    const version = sessionVersion();
    return force ? reading.then(() => { if (isCurrentSession(version)) return refreshAccountPreferences(true); }) : reading;
  }
  if (!force && Date.now() - lastRead < 5000) return Promise.resolve();
  lastRead = Date.now();
  const version = sessionVersion(), started = revision;
  const job = (async () => {
    try {
      const result = await api.preferences();
      if (isCurrentSession(version) && revision === started) accept(result);
    } catch (e) {
      if (!isCurrentSession(version) || revision !== started || handleAuthError(e)) return;
      accountPreferences.error = '계정 설정을 불러오지 못했어요.';
    }
  })().finally(() => { if (reading === job) reading = null; });
  reading = job;
  return job;
}

export function watchAccountPreferences() {
  const refresh = () => { if (document.visibilityState === 'visible') void refreshAccountPreferences(); };
  void refreshAccountPreferences(true);
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

onBeforeLogout(() => saving ?? Promise.resolve());

onSessionChange(() => {
  revision++;
  reading = null;
  saving = null;
  pending = {};
  lastRead = 0;
  const cached = readUserData<AccountPreferences | null>('account-preferences', null);
  confirmed = cached ?? defaults();
  accountPreferences.loaded = cached !== null;
  accountPreferences.saving = false;
  accountPreferences.error = '';
  apply(confirmed);
});
