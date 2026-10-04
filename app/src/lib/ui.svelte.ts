export type Toast = { id: number; text: string; tone: 'info' | 'success' | 'error' | 'alarm'; action?: () => void };

let nextId = 1;
export const toasts = $state<Toast[]>([]);

export function toast(text: string, tone: Toast['tone'] = 'info', ms = 3200, action?: () => void) {
  const id = nextId++;
  toasts.push({ id, text, tone, action });
  setTimeout(() => dismiss(id), ms);
}

export function toastOnce(text: string, tone: Toast['tone'] = 'info', ms = 3200) {
  if (toasts.some((t) => t.text === text)) return;
  toast(text, tone, ms);
}

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}


export type Tab = 'home' | 'calendar' | 'seats' | 'attendance' | 'meals';
export const TABS: { id: Tab; label: string; icon: string }[] = [
  { id: 'home', label: '홈', icon: 'home' },
  { id: 'calendar', label: '캘린더', icon: 'calendar' },
  { id: 'attendance', label: '출결', icon: 'check' },
  { id: 'seats', label: '열람실', icon: 'seat' },
  { id: 'meals', label: '학식', icon: 'bowl' },
];

export const focus = $state({ item: null as string | null, todo: null as number | null, notices: false, timetable: false, seatBuilding: null as string | null, endSeat: null as number | null });
onSessionChange(() => {
  focus.item = null;
  focus.todo = null;
  focus.notices = false;
  focus.timetable = false;
  focus.seatBuilding = null;
  focus.endSeat = null;
  toasts.splice(0);
  route.tab = 'home';
  if (typeof window !== 'undefined') {
    history.replaceState(history.state, '', '#/home');
    window.scrollTo({ top: 0 });
    document.querySelector('.scroller')?.scrollTo({ top: 0 });
  }
});

const current = (): Tab => {
  const id = location.hash.replace(/^#\/?/, '').split('?')[0] as Tab;
  return TABS.some((t) => t.id === id) ? id : 'home';
};

export const route = $state({ tab: current() });

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    route.tab = current();
    window.scrollTo({ top: 0 });
    document.querySelector('.scroller')?.scrollTo({ top: 0 });
  });
}

export function go(tab: Tab) {
  location.hash = `/${tab}`;
}
import { onSessionChange } from './session';

export function openSeats(building: string) {
  focus.seatBuilding = building;
  go('seats');
}
