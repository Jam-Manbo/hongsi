import type { AcademicTerm, CalendarData, Course, SemesterDisplay, Todo } from './types';

export function termLabel(term: AcademicTerm | null | undefined): string {
  if (!term) return '';
  const labels: Record<number, string> = { 10: '1학기', 11: '여름학기', 20: '2학기', 21: '겨울학기' };
  return `${term.year}년 ${labels[term.semester] ?? term.semester}`;
}

export function courseLabel(course: Course, display: SemesterDisplay): string {
  return display === 'all' && course.term ? `${course.name} · ${termLabel(course.term)}` : course.name;
}

export function filterSemesterTodos(todos: Todo[], data: CalendarData | null, display: SemesterDisplay): Todo[] {
  if (display === 'all') return todos;
  const courses = new Set((data?.courses ?? []).filter((course) => data?.semesterDisplay !== 'all'
    || (data.currentTerm && course.term?.year === data.currentTerm.year && course.term?.semester === data.currentTerm.semester))
    .map((course) => course.id));
  return todos.filter((todo) => todo.courseId === null || courses.has(todo.courseId));
}
