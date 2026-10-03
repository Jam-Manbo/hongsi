import { api } from './api';
import { dayKey } from './format';
import { errorText, writeBlocked } from './net.svelte';
import { handleAuthError, todos } from './store.svelte';
import { isStaleSession } from './session';
import { toast, toastOnce } from './ui.svelte';
import type { Todo, TodoInput } from './types';


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
  todos.set(list);
}

function fail(e: unknown, fallback: string) {
  if (!handleAuthError(e)) toastOnce(errorText(e, fallback), 'error');
}

export async function saveTodo(id: number | null, input: TodoInput): Promise<boolean> {
  if (writeBlocked()) return false;
  try {
    const saved = id === null ? await api.createTodo(input) : await api.updateTodo(id, input);
    const list = (todos.data ?? []).filter((t) => t.id !== saved.id);
    replace([...list, saved]);
    toast(id === null ? '할 일을 추가했어요' : '할 일을 수정했어요', 'success', 1800);
    return true;
  } catch (e) {
    fail(e, '저장하지 못했어요');
    return false;
  }
}

export async function toggleTodo(t: Todo) {
  if (writeBlocked()) return;
  const done = t.doneAt === null;
  const before = todos.data ?? [];
  replace(before.map((x) => (x.id === t.id ? { ...x, doneAt: done ? Math.floor(Date.now() / 1000) : null } : x)));
  try {
    const saved = await api.setTodoDone(t.id, done);
    replace((todos.data ?? []).map((x) => (x.id === saved.id ? saved : x)));
  } catch (e) {
    if (isStaleSession(e)) return;
    replace(before);
    fail(e, '저장하지 못했어요');
  }
}

export async function removeTodo(t: Todo) {
  if (writeBlocked('sync', '지울')) return;
  const before = todos.data ?? [];
  replace(before.filter((x) => x.id !== t.id));
  try {
    await api.deleteTodo(t.id);
    toast('할 일을 지웠어요', 'success', 2200);
  } catch (e) {
    if (isStaleSession(e)) return;
    replace(before);
    fail(e, '지우지 못했어요');
  }
}
