const PREFIX = 'hc:';

export function pref<T>(name: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(`${PREFIX}pref:${name}`);
    return raw === null ? fallback : (JSON.parse(raw) as T);
  } catch {
    return fallback;
  }
}

export function setPref<T>(name: string, value: T) {
  localStorage.setItem(`${PREFIX}pref:${name}`, JSON.stringify(value));
}
