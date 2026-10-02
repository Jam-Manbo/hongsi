import { pref, setPref } from './store.svelte';

export type Theme = 'system' | 'light' | 'dark';
export type Midnight = 'prev' | 'same';
export type MealPlace = 'dorm' | 'staff';

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
  mealPlace: pref<MealPlace>('meal-place', 'dorm'),
  alertLeads: pref<number[]>('alert-leads', [60]),
});

export function toggleAlertLead(min: number) {
  const next = settings.alertLeads.includes(min)
    ? settings.alertLeads.filter((m) => m !== min)
    : [...settings.alertLeads, min];
  settings.alertLeads = next.sort((a, b) => b - a);
  setPref('alert-leads', settings.alertLeads);
}

export function setMealPlace(value: MealPlace) {
  settings.mealPlace = value;
  setPref('meal-place', value);
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
