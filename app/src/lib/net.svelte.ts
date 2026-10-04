import { isApp } from './env';
import { withReadTimeout } from './http';
import { toastOnce } from './ui.svelte';

export type Trouble = 'offline' | 'server' | 'school';

export const net = $state({
  online: typeof navigator === 'undefined' || navigator.onLine !== false,
  server: true,
  school: true,
  checking: false,
});

const SERVER = '홍시 서버';

export const TROUBLE_TEXT: Record<Trouble, { title: string; detail: string }> = {
  offline: { title: '인터넷에 연결되어 있지 않아요.', detail: '마지막으로 받은 내용을 보여주고 있어요.' },
  server: isApp
    ? { title: '동기화 서버에 연결할 수 없어요.', detail: '할 일과 좌석 기록은 보기만 할 수 있어요.' }
    : { title: '홍시 서버에 연결할 수 없어요.', detail: '마지막으로 받은 내용을 보여주고 있어요.' },
  school: { title: '학교 서버가 응답하지 않아요.', detail: '마지막으로 받은 내용을 보여주고 있어요.' },
};

export function trouble(): Trouble | null {
  if (!net.online) return 'offline';
  if (!net.server) return 'server';
  if (!net.school) return 'school';
  return null;
}

export function troubleOf(status: number, code: string): Trouble | null {
  if (code === 'offline' || code === 'server_unreachable') return net.online ? 'server' : 'offline';
  if (!isApp && status >= 502 && status <= 504 && code === 'error') return 'server';
  if (code === 'school_unreachable' || code === 'school_error') return 'school';
  return null;
}

type ErrorLike = { status?: unknown; code?: unknown; message?: unknown };

export function errorText(e: unknown, fallback: string): string {
  const err = (e ?? {}) as ErrorLike;
  if (typeof err.status === 'number' && typeof err.code === 'string') {
    const t = troubleOf(err.status, err.code);
    if (t === 'school') return '학교 서버가 응답하지 않아요.';
    if (t) return TROUBLE_TEXT[t].title;
  }
  return typeof err.message === 'string' && err.message ? err.message : fallback;
}


type Listener = (what: 'server' | 'school') => unknown;
const listeners = new Set<Listener>();

export function onReconnect(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

async function reload(what: 'server' | 'school') {
  await Promise.all([...listeners].map((fn) => fn(what)));
}

export function reportServer(ok: boolean) {
  if (ok === net.server) return;
  net.server = ok;
  if (ok) void reload('server');
  schedule();
}

export function reportSchool(ok: boolean) {
  if (ok === net.school) return;
  net.school = ok;
  if (ok) void reload('school');
  schedule();
}


let timer: ReturnType<typeof setTimeout> | undefined;
let attempt = 0;

function schedule() {
  clearTimeout(timer);
  if (!trouble()) {
    attempt = 0;
    return;
  }
  const delay = [10, 20, 30, 60][Math.min(attempt, 3)] * 1000;
  timer = setTimeout(() => {
    attempt += 1;
    if (typeof document === 'undefined' || document.visibilityState === 'visible') void retry();
    else schedule();
  }, delay);
}

async function probeServer(): Promise<boolean> {
  try {
    let ok: boolean;
    if (isApp) {
      const { invoke } = await import('@tauri-apps/api/core');
      const res = await invoke<{ status: number; server?: boolean | null }>('api', {
        method: 'GET',
        path: '/api/health',
        body: null,
      });
      ok = typeof res.server === 'boolean' ? res.server : res.status === 200;
    } else {
      ok = await withReadTimeout(async (signal) => {
        const res = await fetch('/api/health', { cache: 'no-store', credentials: 'same-origin', signal });
        return res.ok && (await res.json().catch(() => null))?.ok === true;
      }, 5_000);
    }
    reportServer(ok);
    return ok;
  } catch {
    reportServer(false);
    return false;
  }
}

export async function retry() {
  if (net.checking) return;
  net.checking = true;
  try {
    if (typeof navigator !== 'undefined') net.online = navigator.onLine !== false;
    if (!net.online) return;
    if (!net.server && !(await probeServer())) return;
    if (!net.school) await reload('school');
  } finally {
    await new Promise((r) => setTimeout(r, 400));
    net.checking = false;
    schedule();
  }
}

export function writeBlocked(kind: 'sync' | 'school' = 'sync', action = '저장할'): boolean {
  let text = '';
  if (!net.online) text = `인터넷에 연결되어 있지 않아 ${action} 수 없어요.`;
  else if (!net.server && (kind === 'sync' || !isApp)) text = `${SERVER}에 연결할 수 없어 ${action} 수 없어요.`;
  if (!text) return false;
  toastOnce(text, 'error');
  void retry();
  return true;
}

if (typeof window !== 'undefined') {
  window.addEventListener('offline', () => {
    net.online = false;
    schedule();
  });
  window.addEventListener('online', () => {
    net.online = true;
    void retry();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible' && trouble()) void retry();
  });
}
