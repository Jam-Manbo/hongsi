import type { CalendarItem, Course, ItemStatus } from './types';

export const RAINBOW = [
  '#ef4444',  
  '#f97316',  
  '#f5a50b',  
  '#84cc16',  
  '#22c55e',  
  '#14b8a6',  
  '#0ea5e9',  
  '#3b82f6',  
  '#6366f1',  
  '#a855f7',  
];

export function courseColors(courses: Course[]): Map<number, string> {
  const terms = new Map<string, Course[]>();
  for (const course of courses) {
    const key = course.term ? `${course.term.year}-${course.term.semester}` : 'other';
    const group = terms.get(key) ?? [];
    group.push(course);
    terms.set(key, group);
  }
  const colors = new Map<number, string>();
  const compare = (a: string, b: string) => a < b ? -1 : a > b ? 1 : 0;
  for (const group of terms.values()) {
    group.sort((a, b) => compare(a.code ?? '', b.code ?? '') || compare(a.name, b.name) || a.id - b.id);
    group.forEach((course, i) => {
      const index = group.length === 1 ? 7 : Math.round(i * (RAINBOW.length - 1) / (group.length - 1));
      colors.set(course.id, RAINBOW[index]);
    });
  }
  return colors;
}

export type Tone = 'ok' | 'warn' | 'danger' | 'muted' | 'info';

const STATUS: Record<ItemStatus, { label: string; tone: Tone }> = {
  submitted: { label: '제출 완료', tone: 'ok' },
  not_submitted: { label: '미제출', tone: 'warn' },
  overdue: { label: '마감 지남', tone: 'danger' },
  unknown: { label: '상태 확인 필요', tone: 'muted' },
  done: { label: '출석 인정', tone: 'ok' },
  partial: { label: '부분 인정', tone: 'warn' },
  missed: { label: '미인정', tone: 'danger' },
  todo: { label: '미시청', tone: 'warn' },
  upcoming: { label: '시청 전', tone: 'info' },
};

export function schoolFinished(item: CalendarItem): boolean {
  return item.status === 'submitted' || item.status === 'done';
}

export function isFinished(item: CalendarItem): boolean {
  return item.done;
}

export function itemStatus(item: CalendarItem): { label: string; tone: Tone } {
  return STATUS[item.status];
}

export function isPending(item: CalendarItem, now = Date.now() / 1000): boolean {
  return !isFinished(item) && (item.due === null || item.due > now)
    && (item.kind !== 'vod' || item.start === null || item.start <= now);
}

export function isOverdue(item: CalendarItem, now = Date.now() / 1000): boolean {
  return !isFinished(item) && item.due !== null && item.due <= now;
}
