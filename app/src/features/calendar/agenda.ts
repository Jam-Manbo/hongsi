import { dueKey } from '../../shared/utils/format';
import type { CalendarItem, Todo } from '../../shared/types';

type AgendaOrder = { day: string | null; allDay: boolean; done: boolean };
export type AgendaEntry = AgendaOrder & (
  | { key: string; kind: 'item'; value: CalendarItem; due: number | null }
  | { key: string; kind: 'todo'; value: Todo; due: number | null }
);

const titles = new Intl.Collator('ko', { numeric: true });

export function agendaItem(value: CalendarItem): AgendaEntry {
  return {
    key: value.key, kind: 'item', value, due: value.due,
    day: value.due === null ? null : dueKey(value.due), allDay: false, done: value.done,
  };
}

export function agendaTodo(value: Todo): AgendaEntry {
  return {
    key: `todo:${value.id}`, kind: 'todo', value,
    due: value.due,
    day: value.due === null ? null : dueKey(value.due),
    allDay: value.due !== null && value.allDay, done: value.doneAt !== null,
  };
}

export function compareAgendaEntries(a: AgendaEntry, b: AgendaEntry): number {
  if (a.day !== b.day) {
    if (a.day === null) return 1;
    if (b.day === null) return -1;
    return a.day < b.day ? -1 : 1;
  }
  if (a.allDay !== b.allDay) return Number(a.allDay) - Number(b.allDay);
  if (a.due !== b.due) {
    if (a.due === null) return 1;
    if (b.due === null) return -1;
    return a.due - b.due;
  }
  return Number(a.done) - Number(b.done)
    || titles.compare(a.value.title, b.value.title)
    || (a.key < b.key ? -1 : a.key > b.key ? 1 : 0);
}

export function agendaEntries(items: CalendarItem[], todos: Todo[]): AgendaEntry[] {
  return [...items.map(agendaItem), ...todos.map(agendaTodo)].sort(compareAgendaEntries);
}
