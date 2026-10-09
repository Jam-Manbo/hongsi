import { api } from '../../shared/api/api';
import { Resource } from '../../shared/state/resource.svelte';
import { calendar } from '../calendar/calendar-resources.svelte';
import { onSessionChange } from '../../shared/state/session';
import type { AcademicTerm, ActiveLectures, AttendanceCourse, AttendanceReceipt, Timetable } from '../../shared/types';

const MIN = 60_000;
export const lectures = new Resource<ActiveLectures>('lectures', () => api.activeLectures(), MIN / 2);
export const attendanceReceipts = new Resource<AttendanceReceipt[]>('attendance-receipts', () => api.attendanceReceipts(), 5_000);
export const attendance = new Resource<AttendanceCourse[]>('attendance', () => api.attendanceStatus(), 10 * MIN);
type SavedTimetable = Timetable & { term?: AcademicTerm | null };
let timetableTermAttempt: string | null = null;
export const timetable = new Resource<SavedTimetable>('timetable', async () => {
  const term = calendar.data?.currentTerm ?? null;
  return { ...await api.timetable(true), term };
}, Infinity);

export function refreshTimetableForTerm(term: AcademicTerm | null | undefined) {
  if (!term || !timetable.data || timetable.loading) return;
  const saved = timetable.data.term;
  if (saved && (saved.year > term.year || (saved.year === term.year && saved.semester >= term.semester))) return;
  const key = `${term.year}-${term.semester}`;
  if (timetableTermAttempt === key) return;
  timetableTermAttempt = key;
  void timetable.refresh();
}

onSessionChange(() => { timetableTermAttempt = null; });
