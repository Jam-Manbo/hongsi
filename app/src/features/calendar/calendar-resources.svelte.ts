import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import { app } from '../auth/auth-state.svelte';
import { normalizeCalendar } from './calendar-data';
import { todos } from './todo-resource.svelte';
import type { CalendarData, SemesterDisplay } from '../../shared/types';

const MIN = 60_000;
let calendarSemesterDisplay: SemesterDisplay = 'current';
export const calendar = new Resource<CalendarData>('calendar-current', async (force) => {
  const data = await api.calendar(force, calendarSemesterDisplay);
  await todos.refresh();
  return data;
}, 5 * MIN, normalizeCalendar);

export function setCalendarSemesterDisplay(display: SemesterDisplay) {
  if (calendarSemesterDisplay === display) return;
  calendarSemesterDisplay = display;
  calendar.setKey(`calendar-${display}`);
  if (app.profile) void calendar.load();
}
