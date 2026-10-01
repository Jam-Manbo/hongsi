import { checkNow } from './classwatch.svelte';
import { attendance, calendar, meals, notices, seats, seatSession, timetable, todos } from './store.svelte';
import { isCurrentSession, onSessionChange, sessionVersion } from './session';
import { TABS, type Tab } from './ui.svelte';

export const refreshState = $state<Record<Tab, { busy: boolean; slow: boolean }>>({
  home: { busy: false, slow: false }, calendar: { busy: false, slow: false },
  attendance: { busy: false, slow: false }, seats: { busy: false, slow: false }, meals: { busy: false, slow: false },
});
const pending = new Map<Tab, Promise<void>>();

export function refreshTab(tab: Tab): Promise<void> {
  const current = pending.get(tab);
  if (current) return current;
  const version = sessionVersion();
  const state = refreshState[tab];
  state.busy = true;
  state.slow = false;
  const slow = setTimeout(() => { if (isCurrentSession(version)) state.slow = true; }, 2500);
  const jobs = {
    home: [seatSession, seats, calendar, meals, todos],
    calendar: [calendar, todos], seats: [seats, seatSession],
    attendance: [attendance, timetable], meals: [meals],
  }[tab];
  if (tab === 'home') { void timetable.load(); void notices.load(); }
  const requests = jobs.map((resource) => resource.load(true));
  if (tab === 'home' || tab === 'attendance') requests.push(checkNow());
  const operation = Promise.all(requests).then(() => {}).finally(() => {
    clearTimeout(slow);
    if (pending.get(tab) === operation) {
      pending.delete(tab);
      state.busy = false;
      state.slow = false;
    }
  });
  pending.set(tab, operation);
  return operation;
}

onSessionChange(() => {
  pending.clear();
  for (const { id } of TABS) { refreshState[id].busy = false; refreshState[id].slow = false; }
});
