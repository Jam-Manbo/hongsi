import { tick } from 'svelte';
import { calendarSync } from './calendar-sync.svelte';
import { closeSheets } from '../components/Sheet.svelte';
import { attendanceWidgetSnapshot, afterSubmit } from './classwatch.svelte';
import { invoke } from '@tauri-apps/api/core';
import { isApp } from './env';
import { courseColors, itemStatus } from './colors';
import { displayedTodos } from './todos.svelte';
import { focus, go, toast } from './ui.svelte';
import type { AttendanceReceipt } from './types';
import { sessionVersion, isCurrentSession } from './session';
import { settings, accountPreferences, type Theme } from './settings.svelte';
import { app, attendanceReceipts, calendar, lectures, seatSession, timetable, todos } from './store.svelte';

export function widgetSnapshot(): string {
  if (!app.profile || !app.account) return '';
  const courses = calendar.data?.courses ?? [];
  const colors = courseColors(courses);
  const course = (id: number | null) => courses.find((c) => c.id === id);
  return JSON.stringify({
    version: 1,
    owner: app.account,
    preferences: { midnight: settings.midnight, showUndated: settings.showUndatedAssignments, timetableDisplay: settings.timetableDisplay, semesterDisplay: settings.semesterDisplay },
    semesterDisplay: calendar.data?.semesterDisplay ?? settings.semesterDisplay,
    attendance: attendanceWidgetSnapshot(),
    errors: { timetable: timetable.error, calendar: calendar.error || todos.error, seats: seatSession.error },
    updatedAt: { preferences: accountPreferences.updatedAt, timetable: timetable.at, calendar: Math.max(calendar.at, todos.at, calendarSync.updatedAt), seats: seatSession.at, lectures: lectures.at },
    slots: (timetable.data?.slots ?? []).map((s) => ({ ...s, color: colors.get(courses.find((c) => c.code === s.code || c.name === s.name)?.id ?? -1) ?? '#3b82f6' })),
    deadlines: [
      ...(calendar.data?.items ?? []).map((i) => ({ key: i.key, title: i.title, course: course(i.courseId)?.name ?? '', due: i.due, start: i.start, done: i.done, kind: i.kind, status: itemStatus(i).label, color: colors.get(i.courseId) ?? '#3b82f6' })),
      ...displayedTodos().map((t) => ({ key: `todo:${t.id}`, title: t.title, course: course(t.courseId)?.name ?? '공통', due: t.due, allDay: t.allDay, start: null, done: t.doneAt !== null, kind: 'todo', status: '', color: colors.get(t.courseId ?? -1) ?? null })),
    ],
    seat: seatSession.data?.session ?? null,
  });
}

export async function publishWidgetTheme(theme: Theme) {
  if (!isApp || !/Android/i.test(navigator.userAgent)) return;
  await invoke('set_widget_theme', { value: theme });
}

export async function publishWidgets(snapshot: string) {
  if (!isApp || !/Android/i.test(navigator.userAgent)) return;
  await invoke('sync_widgets', { value: snapshot }).catch(() => {});
}


let opening = false;
export async function openWidgetIntent() {
  if (!isApp || app.booting || !app.profile || opening || !/Android/i.test(navigator.userAgent)) return;
  opening = true;
  const version = sessionVersion();
  try {
    const { target, changed, receipts, owner: storedOwner } = await invoke<{ receipts: AttendanceReceipt[]; owner: string; target: { kind: string; detail: string; owner: string } | null; changed: boolean }>('widget_intent');
    if (!isCurrentSession(version)) return;
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(app.account ?? ''));
    const owner = [...new Uint8Array(digest)].map((n) => n.toString(16).padStart(2, '0')).join('');
    if (storedOwner === owner) for (const receipt of receipts) afterSubmit({ receipt, synced: false, message: '' });
    if (changed && storedOwner === owner) void Promise.allSettled([lectures.load(true), attendanceReceipts.load(true), seatSession.load(true)]);
    if (!target) return;
    if (owner !== target.owner) { toast('위젯을 새로고침해 주세요.', 'info'); return; }
    if (!closeSheets()) return;
    await tick();
    if (!isCurrentSession(version)) return;
    if (target.kind === 'SEAT') {
      go('seats');
      await seatSession.load(true);
      if (!isCurrentSession(version)) return;
      if (target.detail.startsWith('endSeat:')) {
        const id = Number(target.detail.slice(8));
        if (seatSession.error) toast('다시 시도해 주세요.', 'error');
        else if (id > 0 && seatSession.data?.session?.id === id) focus.endSeat = id;
        else toast('현재 이용 중인 좌석을 확인해 주세요.', 'info');
      }
    }
    else if (target.kind === 'ATTENDANCE') { go('attendance'); await lectures.load(true); }
    else if (target.kind === 'WEEK') { go('attendance'); focus.timetable = true; void timetable.load(); }
    else if (target.kind.startsWith('TODAY')) { go('attendance'); await timetable.load(); }
    else if (target.kind.startsWith('DEADLINES')) {
      go('calendar');
      if (!target.detail.startsWith('deadline:')) return;
      const key = target.detail.replace(/^deadline:/, '');
      const todo = key.startsWith('todo:');
      const resource = todo ? todos : calendar;
      const openDetail = () => {
        if (todo && displayedTodos().some((t) => t.id === Number(key.slice(5)))) {
          focus.todo = Number(key.slice(5));
          return true;
        }
        if (!todo && calendar.data?.items.some((i) => i.key === key)) {
          focus.item = key;
          return true;
        }
        return false;
      };
      if (openDetail()) {
        void resource.load();
        return;
      }
      await resource.load(true);
      if (!isCurrentSession(version)) return;
      if (!openDetail()) toast(resource.error ? '다시 시도해 주세요.' : '표시할 일정이 없어요.', 'info');
    }
  } catch { toast('위젯에 해당하는 화면을 열지 못했어요.', 'error'); }
  finally { opening = false; }
}
