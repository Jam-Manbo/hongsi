import { settings } from './settings.svelte';

const TZ = 'Asia/Seoul';
const DAY = 86_400_000;
export const WEEKDAYS = ['일', '월', '화', '수', '목', '금', '토'];

const formatter = new Intl.DateTimeFormat('en-CA', {
    timeZone: TZ,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hour12: false,
    weekday: 'short',
  });
const readParts = (ms: number) => {
  const p = formatter.formatToParts(ms);
  const get = (t: string) => p.find((x) => x.type === t)?.value ?? '';
  return {
    y: Number(get('year')),
    m: Number(get('month')),
    d: Number(get('day')),
    hh: get('hour') === '24' ? '00' : get('hour'),
    mm: get('minute'),
    wd: ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'].indexOf(get('weekday')),
  };
};

const partCache = new Map<number, ReturnType<typeof readParts>>();
const parts = (ms: number) => {
  const minute = Math.floor(ms / 60_000);
  const cached = partCache.get(minute);
  if (cached) return cached;
  const value = readParts(minute * 60_000);
  if (partCache.size >= 512) partCache.delete(partCache.keys().next().value!);
  partCache.set(minute, value);
  return value;
};

export function dayKey(ms: number): string {
  const { y, m, d } = parts(ms);
  return `${y}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}`;
}

export const todayKey = () => dayKey(Date.now());

export function time(sec: number): string {
  const { hh, mm } = parts(sec * 1000);
  return `${hh}:${mm}`;
}

export function shortDate(sec: number): string {
  const { m, d, wd } = parts(sec * 1000);
  return `${m}/${d}(${WEEKDAYS[wd]})`;
}

export function dateTime(sec: number): string {
  return `${shortDate(sec)} ${time(sec)}`;
}

export function longDay(key: string): string {
  const [y, m, d] = key.split('-').map(Number);
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  return `${m}월 ${d}일 ${WEEKDAYS[wd]}요일`;
}

function daysBetween(fromKey: string, toKey: string): number {
  const toUtc = (k: string) => {
    const [y, m, d] = k.split('-').map(Number);
    return Date.UTC(y, m - 1, d);
  };
  return Math.round((toUtc(toKey) - toUtc(fromKey)) / DAY);
}

const isMidnight = (sec: number) => {
  if (settings.midnight === 'same') return false;
  const { hh, mm } = parts(sec * 1000);
  return hh === '00' && mm === '00';
};

export function dueKey(sec: number): string {
  return dayKey((isMidnight(sec) ? sec - 60 : sec) * 1000);
}

export function dueTime(sec: number): string {
  return isMidnight(sec) ? '24:00' : time(sec);
}

export function dueDate(sec: number): string {
  return shortDate(isMidnight(sec) ? sec - 60 : sec);
}

export function dueDateTime(sec: number): string {
  return `${dueDate(sec)} ${dueTime(sec)}`;
}

export function dday(sec: number, now: number): { label: string; tone: 'past' | 'today' | 'soon' | 'later' } {
  const diff = daysBetween(dayKey(now), dueKey(sec));
  if (sec * 1000 < now) return { label: diff === 0 ? '오늘 마감됨' : `${-diff}일 지남`, tone: 'past' };
  if (diff === 0) return { label: 'D-DAY', tone: 'today' };
  return { label: `D-${diff}`, tone: diff <= 3 ? 'soon' : 'later' };
}

export function duration(ms: number): string {
  const total = Math.max(0, Math.round(ms / 60_000));
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m}분`;
  return m === 0 ? `${h}시간` : `${h}시간 ${m}분`;
}

export function ago(sec: number): string {
  const min = Math.round((Date.now() - sec * 1000) / 60_000);
  if (min < 1) return '방금 전';
  if (min < 60) return `${min}분 전`;
  const h = Math.round(min / 60);
  return h < 24 ? `${h}시간 전` : `${Math.round(h / 24)}일 전`;
}

export function monthCells(year: number, month: number): { key: string; inMonth: boolean }[] {
  const first = new Date(Date.UTC(year, month - 1, 1));
  const start = new Date(first.getTime() - first.getUTCDay() * DAY);
  return Array.from({ length: 42 }, (_, i) => {
    const d = new Date(start.getTime() + i * DAY);
    const key = `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, '0')}-${String(d.getUTCDate()).padStart(2, '0')}`;
    return { key, inMonth: d.getUTCMonth() === month - 1 };
  });
}

export function hourNow(): number {
  return Number(parts(Date.now()).hh);
}

export function sentenceLines(text: string): string {
  return text.replace(/([.!?]) +(?=[^\s·])/g, '$1\n');
}
