let version = 0;
let user: string | null = null;
const listeners = new Set<() => void>();

export class StaleSessionError extends Error {
  constructor() {
    super('이전 로그인에서 시작한 요청이에요');
  }
}

export const sessionUser = () => user;
export const sessionVersion = () => version;
export const isCurrentSession = (value: number) => value === version;
export const isStaleSession = (e: unknown) => e instanceof StaleSessionError;

export async function inSession<T>(job: (check: () => void) => Promise<T>): Promise<T> {
  const started = version;
  const check = () => { if (!isCurrentSession(started)) throw new StaleSessionError(); };
  try {
    const result = await job(check);
    check();
    return result;
  } catch (e) {
    check();
    throw e;
  }
}

export function onSessionChange(fn: () => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function setSessionUser(id: string | null) {
  user = id?.trim().toUpperCase() || null;
  version += 1;
  listeners.forEach((fn) => fn());
}

const prefix = () => user ? `hc:user:${encodeURIComponent(user)}:` : null;

export function readUserData<T>(key: string, fallback: T): T {
  try {
    const p = prefix();
    const raw = p ? localStorage.getItem(p + key) : null;
    return raw === null ? fallback : JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

export function writeUserData<T>(key: string, value: T) {
  try {
    const p = prefix();
    if (p) localStorage.setItem(p + key, JSON.stringify(value));
  } catch {
  }
}

export function clearUserData() {
  try {
    const p = prefix();
    if (p) Object.keys(localStorage).filter((k) => k.startsWith(p)).forEach((k) => localStorage.removeItem(k));
  } catch {   }
}

export function clearLegacyData() {
  try {
    for (const key of Object.keys(localStorage)) {
      if (key.startsWith('hc:') && !key.startsWith('hc:pref:') && !key.startsWith('hc:user:')) localStorage.removeItem(key);
    }
    localStorage.removeItem('hc:pref:notices-seen');
  } catch {   }
}
