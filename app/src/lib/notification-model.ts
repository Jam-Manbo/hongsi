export type NotificationTarget =
  | { kind: 'item'; key: string }
  | { kind: 'todo'; id: number }
  | { kind: 'seat'; id: number }
  | { kind: 'notices' };
export type NotificationIntent = { version: 1; account: string; target: NotificationTarget; at: number };
export type Reminder = { key: string; at: number; title: string; body: string; target: NotificationTarget };
export type Channel = 'seat' | 'due';
export const ID_BASE: Record<Channel, number> = { seat: 7000, due: 8000 };
export const isReminderId = (id: number) => Object.values(ID_BASE).some((base) => id >= base && id < base + 500);
export type Permission = 'unknown' | 'granted' | 'denied' | 'default' | 'unsupported';
export type ChannelStatus = { planned: number; scheduled: number; deferred: number; next: number | null; checkedAt: number | null; error: string };
export const emptyStatus = (): ChannelStatus => ({ planned: 0, scheduled: 0, deferred: 0, next: null, checkedAt: null, error: '' });

export function parseIntent(value: unknown): NotificationIntent | null {
  if (typeof value === 'string') { try { value = JSON.parse(value); } catch { return null; } }
  if (!value || typeof value !== 'object') return null;
  const x = value as NotificationIntent, t = x.target;
  if (x.version !== 1 || typeof x.account !== 'string' || !x.account || x.account.length > 128 || !Number.isFinite(x.at) || !t) return null;
  if (t.kind === 'notices') return x;
  if (t.kind === 'item' && typeof t.key === 'string' && /^(assign|vod):\d+$/.test(t.key)) return x;
  if ((t.kind === 'todo' || t.kind === 'seat') && Number.isSafeInteger(t.id) && t.id > 0) return x;
  return null;
}

export function reminderPlan(reminders: Reminder[], limit: number, now = Date.now(), horizon = Infinity) {
  const unique = new Map<string, Reminder>();
  for (const r of reminders) if (Number.isFinite(r.at) && r.at > now) unique.set(r.key, r);
  const all = [...unique.values()].sort((a, b) => a.at - b.at || a.key.localeCompare(b.key));
  const selected = all.filter((r) => r.at - now <= horizon).slice(0, limit);
  return { all, selected, deferred: all.length - selected.length };
}
