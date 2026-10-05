import { SvelteSet } from 'svelte/reactivity';
import { api } from './api';
import { dayKey } from './format';
import { errorText, writeBlocked } from './net.svelte';
import { calendar, handleAuthError, todos } from './store.svelte';
import { settings } from './settings.svelte';
import { filterSemesterTodos } from './semester';
import { isCurrentSession, onSessionChange, sessionVersion } from './session';
import { toast, toastOnce } from './ui.svelte';
import type { Todo, TodoInput } from './types';

export function displayedTodos(): Todo[] {
  return filterSemesterTodos(todos.data ?? [], calendar.data, settings.semesterDisplay);
}


export function todoKey(t: Todo): string | null {
  return t.dueAt === null ? null : dayKey(t.dueAt * 1000);
}

export function todoDeadline(t: Todo): number | null {
  return t.dueAt === null ? null : t.dueAt + (t.allDay ? 86_400 : 0);
}

export function isTodoPending(t: Todo, now = Date.now() / 1000): boolean {
  const due = todoDeadline(t);
  return t.doneAt === null && (due === null || due > now);
}

export function toUnix(date: string, time = ''): number {
  const [y, m, d] = date.split('-').map(Number);
  const [hh, mm] = time ? time.split(':').map(Number) : [0, 0];
  return Math.floor((Date.UTC(y, m - 1, d, hh, mm) - 9 * 3600_000) / 1000);
}

export function fromUnix(sec: number): { date: string; time: string } {
  const k = new Date(sec * 1000 + 9 * 3600_000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return {
    date: `${k.getUTCFullYear()}-${pad(k.getUTCMonth() + 1)}-${pad(k.getUTCDate())}`,
    time: `${pad(k.getUTCHours())}:${pad(k.getUTCMinutes())}`,
  };
}

function replace(list: Todo[]) {
  todos.set([...list].sort((a, b) => (a.dueAt ?? Infinity) - (b.dueAt ?? Infinity) || a.id - b.id));
}

function fail(e: unknown, fallback: string) {
  if (!handleAuthError(e)) toastOnce(errorText(e, fallback), 'error');
}

export const pendingTodos = new SvelteSet<number>();
onSessionChange(() => pendingTodos.clear());

function begin(id: number | null) {
  if (id !== null && pendingTodos.has(id)) return null;
  const version = sessionVersion();
  if (id !== null) pendingTodos.add(id);
  const finish = todos.beginMutation();
  return {
    current: () => isCurrentSession(version),
    finish() {
      finish();
      if (isCurrentSession(version) && id !== null) pendingTodos.delete(id);
    },
  };
}

export async function saveTodo(id: number | null, input: TodoInput): Promise<boolean> {
  if (writeBlocked()) return false;
  const change = begin(id);
  if (!change) return false;
  try {
    const saved = id === null ? await api.createTodo(input) : await api.updateTodo(id, input);
    if (!change.current()) return false;
    replace([...(todos.data ?? []).filter((t) => t.id !== saved.id), saved]);
    toast(id === null ? '할 일을 추가했어요.' : '할 일을 수정했어요.', 'success', 1800);
    return true;
  } catch (e) {
    if (change.current()) fail(e, '저장하지 못했어요.');
    return false;
  } finally {
    change.finish();
  }
}

export async function toggleTodo(t: Todo) {
  if (writeBlocked()) return;
  const current = todos.data?.find((x) => x.id === t.id);
  if (!current) return;
  const change = begin(t.id);
  if (!change) return;
  const before = current.doneAt, done = before === null;
  replace((todos.data ?? []).map((x) => x.id === t.id ? { ...x, doneAt: done ? Math.floor(Date.now() / 1000) : null } : x));
  try {
    const saved = await api.setTodoDone(t.id, done);
    if (change.current()) replace((todos.data ?? []).map((x) => x.id === saved.id ? saved : x));
  } catch (e) {
    if (!change.current()) return;
    replace((todos.data ?? []).map((x) => x.id === t.id ? { ...x, doneAt: before } : x));
    fail(e, '저장하지 못했어요.');
  } finally {
    change.finish();
  }
}

export async function removeTodo(t: Todo): Promise<boolean> {
  if (writeBlocked('sync', '지울')) return false;
  const before = todos.data?.find((x) => x.id === t.id);
  if (!before) return false;
  const change = begin(t.id);
  if (!change) return false;
  replace((todos.data ?? []).filter((x) => x.id !== t.id));
  try {
    await api.deleteTodo(t.id);
    if (!change.current()) return false;
    toast('할 일을 지웠어요.', 'success', 2200);
    return true;
  } catch (e) {
    if (change.current()) {
      if (!todos.data?.some((x) => x.id === t.id)) replace([...(todos.data ?? []), before]);
      fail(e, '지우지 못했어요.');
    }
    return false;
  } finally {
    change.finish();
  }
}
