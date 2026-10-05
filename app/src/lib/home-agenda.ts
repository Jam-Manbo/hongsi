import { agendaItem, agendaTodo, compareAgendaEntries, type AgendaEntry } from './agenda';
import { dayKey, dueKey } from './format';
import type { CalendarItem, Todo } from './types';

export type HomeEntry = AgendaEntry;

export function homeAgenda(items: CalendarItem[], todos: Todo[], nowMs: number, showUndatedAssignments = false) {
  const now = nowMs / 1000;
  const today = dayKey(nowMs);
  const groups: Record<'today' | 'overdue' | 'upcoming' | 'undated', HomeEntry[]> = {
    today: [], overdue: [], upcoming: [], undated: [],
  };
  for (const value of items) {
    if (value.done || (value.start !== null && value.start > now)) continue;
    const entry = agendaItem(value);
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
    const entry = agendaTodo(value);
    const { due } = entry;
    if (due === null) groups.undated.push(entry);
    else if (due <= now) groups.overdue.push(entry);
    else groups[dueKey(due) <= today ? 'today' : 'upcoming'].push(entry);
  }
  for (const entries of Object.values(groups)) {
    entries.sort(compareAgendaEntries);
  }
  return groups;
}
