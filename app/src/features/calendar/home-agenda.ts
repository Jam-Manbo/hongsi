import { agendaItem, agendaTodo, compareAgendaEntries, type AgendaEntry } from './agenda';
import { dayKey } from '../../shared/utils/format';
import type { CalendarItem, Todo } from '../../shared/types';

export type HomeEntry = AgendaEntry;

export function homeAgenda(items: CalendarItem[], todos: Todo[], nowMs: number, showUndatedAssignments = false) {
  const now = nowMs / 1000;
  const today = dayKey(nowMs);
  const groups: Record<'today' | 'upcoming' | 'undated', HomeEntry[]> = {
    today: [], upcoming: [], undated: [],
  };
  for (const entry of [...items.map(agendaItem), ...todos.map(agendaTodo)]) {
    if (entry.day === today) {
      groups.today.push(entry);
    }
    if (!entry.done && entry.due !== null && entry.due > now) {
      groups.upcoming.push(entry);
    } else if (!entry.done && entry.due === null
      && (entry.kind === 'todo' || (entry.value.kind === 'assignment' && showUndatedAssignments))) {
      groups.undated.push(entry);
    }
  }
  for (const entries of Object.values(groups)) {
    entries.sort(compareAgendaEntries);
  }
  return groups;
}
