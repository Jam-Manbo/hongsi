import { dayKey, dueKey } from './format';
import type { CalendarItem, Todo } from './types';

export type HomeEntry =
  | { key: string; kind: 'item'; value: CalendarItem; due: number | null }
  | { key: string; kind: 'todo'; value: Todo; due: number | null };

export function homeAgenda(items: CalendarItem[], todos: Todo[], nowMs: number, showUndatedAssignments = false) {
  const now = nowMs / 1000;
  const today = dayKey(nowMs);
  const groups: Record<'today' | 'overdue' | 'upcoming' | 'undated', HomeEntry[]> = {
    today: [], overdue: [], upcoming: [], undated: [],
  };
  for (const value of items) {
    if (value.done || value.status === 'upcoming' || (value.start !== null && value.start > now)) continue;
    const entry: HomeEntry = { key: value.key, kind: 'item', value, due: value.due };
    if (value.due === null) {
      if (value.kind === 'assignment' && showUndatedAssignments) groups.undated.push(entry);
    } else if (value.due <= now) {
      if (value.kind === 'assignment' && (value.lateUntil === null || value.lateUntil > now)) groups.overdue.push(entry);
    } else {
      groups[dueKey(value.due) <= today ? 'today' : 'upcoming'].push(entry);
    }
  }
  for (const value of todos) {
    if (value.doneAt !== null) continue;
    const due = value.dueAt === null ? null : value.dueAt + (value.allDay ? 86_400 : 0);
    const entry: HomeEntry = { key: `todo:${value.id}`, kind: 'todo', value, due };
    if (due === null) groups.undated.push(entry);
    else if (due <= now) groups.overdue.push(entry);
    else groups[dayKey(value.dueAt! * 1000) <= today ? 'today' : 'upcoming'].push(entry);
  }
  for (const entries of Object.values(groups)) {
    entries.sort((a, b) => (a.due ?? Infinity) - (b.due ?? Infinity) || a.key.localeCompare(b.key));
  }
  return groups;
}
