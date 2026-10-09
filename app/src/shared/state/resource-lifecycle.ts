import { isApp } from '../../platform/env';
import { app } from '../../features/auth/auth-state.svelte';
import { onSessionChange } from './session';
import { onReconnect } from '../api/net.svelte';
import { calendar } from '../../features/calendar/calendar-resources.svelte';
import { seats, seatSession } from '../../features/seats/seat-resources.svelte';
import { meals } from '../../features/meals/meal-resource.svelte';
import { lectures, attendance, attendanceReceipts, timetable } from '../../features/attendance/attendance-resources.svelte';
import { todos } from '../../features/calendar/todo-resource.svelte';
import { notices } from '../../features/classroom/notice-resource.svelte';

export const allResources = [calendar, seats, seatSession, meals, lectures, attendance, attendanceReceipts, timetable, todos, notices];
onSessionChange(() => {
  allResources.forEach((r) => r.restore());
});

onReconnect((what) => {
  if (!app.profile || app.loggingOut) return;
  const jobs = allResources.filter((r) => r.error !== null && (r !== timetable || r.data === null) && (what === 'server' || r.trouble === 'school'));
  if (what === 'server' && isApp && calendar.data && !jobs.includes(calendar)) jobs.push(calendar);
  return Promise.all(jobs.map((r) => r.load(true)));
});
