import { dayKey, dueDate, dueDateTime, dueKey, dueTime } from '../../shared/utils/format';
import { scheduleReminders, type Reminder } from './notify';
import type { CalendarItem, Course, Todo } from '../../shared/types';

const LEAD_TEXT: Record<number, string> = { 1440: '하루 전', 180: '3시간 전', 60: '1시간 전', 10: '10분 전' };
const leadText = (min: number) => LEAD_TEXT[min] ?? (min % 60 === 0 ? `${min / 60}시간 전` : `${min}분 전`);

function withCourse(title: string, course: string | undefined): string {
  const head = course?.split(/[\s(]/)[0];
  return head && !title.startsWith(head) ? `${head} · ${title}` : title;
}

function itemWhen(due: number, at: number): string {
  return dayKey(at) === dueKey(due) ? dueTime(due) : dueDateTime(due);
}

function todoWhen(t: Todo, at: number): string {
  const due = t.due as number;
  const sameDay = dayKey(at) === dueKey(due);
  if (t.allDay) return sameDay ? '오늘' : dueDate(due);
  return itemWhen(due, at);
}

function dueReminders(items: CalendarItem[], todos: Todo[], courses: Course[], leads: number[], now = Date.now()): Reminder[] {
  const names = new Map(courses.map((c) => [c.id, c.name]));
  const out: Reminder[] = [];
  for (const i of items) {
    if (i.done || i.alert === false || i.due === null) continue;
    const what = i.kind === 'vod' ? '온라인 강의' : '과제';
    for (const min of i.alertLeads ?? leads) {
      const at = i.due * 1000 - min * 60_000;
      if (at <= now) continue;
      out.push({
        key: `item:${i.key}:${i.due}:${min}`,
        target: { kind: 'item', key: i.key },
        at,
        title: min === 0 ? `${what} 마감 시간이에요.` : `${what} 마감 ${leadText(min)}이에요.`,
        body: `${withCourse(i.title, names.get(i.courseId))} · ${itemWhen(i.due, at)}까지`,
      });
    }
  }
  for (const t of todos) {
    if (t.doneAt !== null || t.notify === false || t.due === null) continue;
    const deadline = t.due * 1000;
    for (const min of t.alertLeads ?? leads) {
      const at = deadline - min * 60_000;
      if (at <= now) continue;
      const course = t.courseId === null ? undefined : names.get(t.courseId);
      out.push({
        key: `todo:${t.id}:${deadline}:${min}`,
        target: { kind: 'todo', id: t.id },
        at,
        title: min === 0 ? '할 일 마감 시간이에요.' : `할 일 마감 ${leadText(min)}이에요.`,
        body: `${course ? `${course} · ` : ''}${t.title} · ${todoWhen(t, at)}까지`,
      });
    }
  }
  return out;
}

export function syncDueReminders(items: CalendarItem[], todos: Todo[], courses: Course[], leads: number[]) {
  void scheduleReminders('due', dueReminders(items, todos, courses, leads));
}
