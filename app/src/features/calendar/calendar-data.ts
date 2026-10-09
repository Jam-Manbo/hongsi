import type { CalendarData } from '../../shared/types';

export function normalizeCalendar(data: CalendarData): CalendarData {
  const keys = new Set<string>();
  const ids = new Set<number>();
  return {
    ...data,
    items: data.items.filter((item) => {
      if (keys.has(item.key)) return false;
      keys.add(item.key);
      return true;
    }),
    courses: data.courses.filter((course) => {
      if (ids.has(course.id)) return false;
      ids.add(course.id);
      return true;
    }),
  };
}
