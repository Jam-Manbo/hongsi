import { api } from './api';
import { dayKey, todayKey } from './format';
import { attendance, attendanceReceipts, handleAuthError, lectures, timetable } from './store.svelte';
import { isCurrentSession, onSessionChange, readUserData, sessionVersion, writeUserData } from './session';
import type { ActiveLecture, AttendanceCourse, AttendanceReceipt, AttendanceSubmission, ClassSlot, MarkKind } from './types';

const WATCH_BEFORE = 3 * 60_000;
const WATCH_AFTER = 10 * 60_000;
export const POLL_MS = 10_000;
const STATUS_MS = 60_000;
const MIN_SPIN_MS = 700;
const KST = 9 * 3600_000;
const STORE = 'attendance-confirmed';

export type TodayClass = ClassSlot & { at: number };
export type Mark = { label: string; kind: MarkKind; source: 'school' | 'app' };
type CourseRef = { code: string | null; name: string; at?: number; lectureKey?: string };
type SavedReceipt = { receipt: AttendanceReceipt; synced: boolean };
const LABEL: Partial<Record<MarkKind, string>> = { present: '출석', late: '지각', excused: '공결', absent: '결석' };
const ATTENDED: MarkKind[] = ['present', 'late', 'excused'];
const norm = (s: string) => s.replace(/\(\*\)|\s/g, '');
const keyOf = (c: CourseRef) => `${c.at ? dayKey(c.at) : todayKey()}|${c.code || norm(c.name)}|${c.at ?? c.lectureKey ?? 'course'}`;
const validReceipt = (r: AttendanceReceipt) => r?.date === todayKey() && ATTENDED.includes(r.kind)
  && typeof r.lecture?.key === 'string' && Number.isSafeInteger(r.confirmedAt) && Math.abs(r.confirmedAt) <= 8.64e15 && dayKey(r.confirmedAt) === todayKey();
function loadSaved(): SavedReceipt[] {
  const saved = readUserData<unknown>(STORE, []);
  return Array.isArray(saved) ? saved.filter((entry) => entry && validReceipt(entry.receipt) && typeof entry.synced === 'boolean') : [];
}
export const classWatch = $state({
  current: null as TodayClass | null,
  nextAt: 0, polling: false, now: Date.now(), day: todayKey(),
  receipts: loadSaved(),
  seenOpen: [] as string[],
  school: {} as Record<string, { course: AttendanceCourse; at: number }>,
  shareError: '',
});

export function todayClasses(slots: ClassSlot[], now = Date.now()): TodayClass[] {
  const k = new Date(now + KST);
  const weekday = (k.getUTCDay() + 6) % 7;
  const midnight = Date.UTC(k.getUTCFullYear(), k.getUTCMonth(), k.getUTCDate()) - KST;
  return slots
    .filter((s) => s.weekday === weekday)
    .map((s) => {
      const [h, m] = s.start.split(':').map(Number);
      return { ...s, at: midnight + (h * 60 + m) * 60_000 };
    })
    .sort((a, b) => a.at - b.at);
}

export function classSessions(c: TodayClass): (TodayClass & { round: number })[] {
  return c.periods.map((period, index) => {
    const offset = (period - c.periods[0]) * 60;
    const [h, m] = c.start.split(':').map(Number);
    const minutes = h * 60 + m + offset;
    const start = `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
    return { ...c, periods: [period], start, at: c.at + offset * 60_000, round: index + 1 };
  });
}

export const todaySessions = (slots: ClassSlot[], now = Date.now()) => todayClasses(slots, now).flatMap(classSessions);

export const inWindow = (c: TodayClass, now = Date.now()) => now >= c.at - WATCH_BEFORE && now <= c.at + WATCH_AFTER;

const sameClass = (a: TodayClass | null, b: TodayClass | null) => a?.at === b?.at && a?.name === b?.name && a?.code === b?.code;

export const nextClass = (now: number) => todayClasses(timetable.data?.slots ?? [], now).find((c) => c.at > now) ?? null;

export const periodLabel = (c: ClassSlot) =>
  c.periods.length > 1 ? `${c.periods[0]}–${c.periods[c.periods.length - 1]}교시` : `${c.periods[0]}교시`;

export function sameCourse(l: { code?: string | null; name: string }, c: CourseRef): boolean {
  if (l.code && c.code) return l.code === c.code;
  return norm(l.name) === norm(c.name);
}

type AttendedMark = Mark & { kind: 'present' | 'late' | 'excused' };
export const isAttended = (m: Mark | null): m is AttendedMark => !!m && ATTENDED.includes(m.kind);
export const markTitle = (m: Mark) => m.kind === 'present' ? '출석 완료' : `${m.label} 처리됨`;

function schoolMark(c: CourseRef, course: AttendanceCourse): Mark | null {
  if (!course.published || c.at === undefined || dayKey(c.at) !== todayKey()) return null;
  const sessions = todaySessions(timetable.data?.slots ?? []).filter((s) => sameCourse(s, c));
  const index = sessions.findIndex((s) => s.at === c.at);
  const k = new Date(c.at + KST);
  const date = `${k.getUTCMonth() + 1}/${k.getUTCDate()}`;
  const entries = course.weeks.flatMap((w) => w.sessions).filter((s) => s.date === date);
  if (index < 0 || entries.length !== sessions.length) return null;
  const entry = entries[index];
  if (LABEL[entry.kind]) return { label: LABEL[entry.kind]!, kind: entry.kind, source: 'school' };
  return entry.kind === 'other' && entry.mark ? { label: entry.mark, kind: entry.kind, source: 'school' } : null;
}

function lectureRef(l: ActiveLecture, now = Date.now()): CourseRef {
  const classes = todaySessions(timetable.data?.slots ?? [], now).filter((c) => sameCourse(l, c));
  const time = /(?:^|\D)(\d{1,2}):(\d{2})/.exec(l.time);
  const start = time ? `${time[1].padStart(2, '0')}:${time[2]}` : null;
  const matches = start ? classes.filter((c) => c.start === start) : classes.filter((c) => inWindow(c, now));
  return matches.length === 1 ? { ...matches[0], lectureKey: l.key } : { code: l.code ?? null, name: l.name, lectureKey: l.key };
}

export function lectureSession(l: ActiveLecture, now = classWatch.now): TodayClass | null {
  const sessions = todaySessions(timetable.data?.slots ?? [], now).filter((c) => sameCourse(l, c));
  const clock = /(?:^|\D)(\d{1,2}):(\d{2})/.exec(l.time);
  const period = /^\s*([월화수목금토일])\s*(\d{1,2})(?:교시)?\s*$/.exec(l.time);
  const matches = clock
    ? sessions.filter((c) => c.start === `${clock[1].padStart(2, '0')}:${clock[2]}`)
    : period ? sessions.filter((c) => c.weekday === '월화수목금토일'.indexOf(period[1]) && c.periods[0] === Number(period[2]))
    : sessions.filter((c) => inWindow(c, now));
  return matches.length === 1 ? matches[0] : !clock && !period && sessions.length === 1 ? sessions[0] : null;
}

export function classScheduleLabel(c: ClassSlot | null, fallback = ''): string {
  if (!c) return fallback.trim();
  const period = `${'월화수목금토일'[c.weekday] ?? ''}${c.periods[0] ?? ''}`;
  return [period, c.start ? `${c.start} 수업` : '', c.room ?? ''].filter(Boolean).join(' ');
}

export function markFor(c: CourseRef): Mark | null {
  if (c.at !== undefined && dayKey(c.at) !== todayKey()) return null;
  const matches = (attendance.data ?? []).filter((course) => sameCourse(course, c));
  const listed = matches.length === 1 ? matches[0] : undefined;
  const latest = classWatch.school[c.code ?? listed?.code ?? ''];
  const course = latest && latest.at >= attendance.at ? latest.course : listed;
  const official = course ? schoolMark(c, course) : null;
  if (official) return official;
  const receipts = [...(attendanceReceipts.data ?? []), ...classWatch.receipts.map((entry) => entry.receipt)];
  const receipt = receipts.filter((r) => validReceipt(r) && (c.lectureKey === r.lecture.key || keyOf(lectureRef(r.lecture, r.confirmedAt)) === keyOf(c)))
    .sort((a, b) => b.confirmedAt - a.confirmedAt)[0];
  return receipt ? { label: LABEL[receipt.kind]!, kind: receipt.kind, source: 'app' } : null;
}
export const lectureMark = (l: ActiveLecture) => markFor(lectureRef(l));

export function sessionState(c: TodayClass): { label: string; cls: string } {
  const m = markFor(c);
  if (m) return { label: m.label, cls: m.kind === 'absent' ? 'danger' : m.kind === 'late' ? 'warn' : isAttended(m) ? 'ok' : '' };
  const fresh = !lectures.error && lectures.at >= classWatch.now - 2 * POLL_MS;
  const listed = fresh && lectures.data?.items.some((l) => keyOf(lectureRef(l)) === keyOf(c));
  if (listed) return { label: '출석 가능', cls: 'primary' };
  if (fresh && classWatch.seenOpen.includes(keyOf(c))) return { label: '확인 불가', cls: '' };
  if (c.at > classWatch.now) return { label: inWindow(c, classWatch.now) ? '확인 중' : '예정', cls: inWindow(c, classWatch.now) ? 'primary' : '' };
  if (inWindow(c, classWatch.now) && !fresh && !lectures.error) return { label: '확인 중', cls: 'primary' };
  return { label: '확인 불가', cls: '' };
}

export function attendanceWidgetSnapshot() {
  return {
    date: todayKey(),
    loaded: lectures.data !== null,
    error: lectures.error,
    timetableLoaded: timetable.data !== null,
    timetableError: timetable.error,
    checkedAt: lectures.at,
    sessions: todaySessions(timetable.data?.slots ?? []).map((session) => ({
      ...session,
      identity: keyOf(session),
      mark: markFor(session),
      seenOpen: classWatch.seenOpen.includes(keyOf(session)),
    })),
    active: (lectures.data?.items ?? []).map((lecture) => ({
      key: lecture.key, name: lecture.name, time: lecture.time, code: lecture.code,
      scheduleLabel: classScheduleLabel(lectureSession(lecture), lecture.time),
      identity: keyOf(lectureRef(lecture)), mark: lectureMark(lecture),
    })),
  };
}

function persist() { writeUserData(STORE, $state.snapshot(classWatch.receipts)); }
let users = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let statusAt = 0;
let sharedAt = 0;
let pendingCheck: Promise<void> | null = null;
let pendingSync: Promise<void> | null = null;

export function useClassWatch(): () => void {
  users++;
  if (users === 1) {
    timer = setInterval(tick, 1000);
    document.addEventListener('visibilitychange', onVisibility);
    window.addEventListener('online', onVisibility);
    void attendance.load();
  }
  void checkNow();
  return () => {
    users--;
    if (users === 0) {
      clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
      window.removeEventListener('online', onVisibility);
    }
  };
}

async function syncReceipts() {
  if (pendingSync) return pendingSync;
  const version = sessionVersion();
  sharedAt = Date.now();
  const task = (async () => {
    let failed = false;
    for (const entry of classWatch.receipts.filter((entry) => !entry.synced && validReceipt(entry.receipt))) {
      try {
        await api.shareAttendanceReceipt(entry.receipt);
        if (!isCurrentSession(version)) return;
        entry.synced = true;
        persist();
      } catch (e) {
        if (!isCurrentSession(version)) return;
        if (handleAuthError(e)) return;
        failed = true;
      }
    }
    if (!isCurrentSession(version)) return;
    classWatch.shareError = failed ? '출석은 확인했지만 다른 기기에 기록을 공유하지 못했어요. 서버에 연결되면 다시 시도할게요.' : '';
    await attendanceReceipts.load(true);
  })().finally(() => { if (pendingSync === task) pendingSync = null; });
  pendingSync = task;
  return task;
}

export function checkNow(): Promise<void> {
  if (pendingCheck) return pendingCheck;
  tick(false);
  classWatch.nextAt = Date.now() + POLL_MS;
  const task = poll().finally(() => { if (pendingCheck === task) pendingCheck = null; });
  pendingCheck = task;
  return task;
}
function onVisibility() {
  if (document.visibilityState === 'visible') {
    sharedAt = 0;
    void checkNow();
  }
}
function tick(automatic = true) {
  const now = Date.now();
  classWatch.now = now;
  if (classWatch.day !== todayKey()) {
    classWatch.day = todayKey();
    classWatch.receipts = loadSaved();
    classWatch.school = {}; classWatch.seenOpen = [];
    persist();
    sharedAt = 0; statusAt = 0;
  }
  const cur = todaySessions(timetable.data?.slots ?? [], now).find((c) => inWindow(c, now)) ?? null;
  if (!sameClass(cur, classWatch.current)) {
    classWatch.current = cur;
    classWatch.nextAt = Math.max(now, lectures.at + POLL_MS);
    statusAt = 0;
  }
  const active = dayKey(lectures.at) === todayKey() ? (lectures.data?.items ?? []) : [];
  if (!lectures.error && lectures.at >= now - 2 * POLL_MS) {
    const seen = new Set(classWatch.seenOpen);
    for (const lecture of active) seen.add(keyOf(lectureRef(lecture)));
    if (seen.size !== classWatch.seenOpen.length) classWatch.seenOpen = [...seen];
  }
  if (!automatic || document.visibilityState !== 'visible') return;
  const open = active.filter((lecture) => !isAttended(lectureMark(lecture)));
  if (now - sharedAt >= (cur || open.length ? POLL_MS : 30_000)) void syncReceipts();
  const watched = cur ?? (open[0] ? lectureRef(open[0]) : null);
  if (watched?.code && (statusAt === 0 || now - statusAt >= STATUS_MS)) {
    statusAt = now;
    void checkSchool(watched);
  }
  if (((cur && !isAttended(markFor(cur))) || open.length || lectures.error !== null) && now >= classWatch.nextAt && !classWatch.polling) void checkNow();
}
async function poll() {
  const version = sessionVersion();
  classWatch.polling = true;
  const started = Date.now();
  await Promise.all([lectures.load(true), syncReceipts()]);
  if (!isCurrentSession(version)) return;
  tick(false);
  const rest = MIN_SPIN_MS - (Date.now() - started);
  if (rest > 0) await new Promise((resolve) => setTimeout(resolve, rest));
  if (isCurrentSession(version)) classWatch.polling = false;
}
async function checkSchool(c: CourseRef) {
  if (!c.code) return;
  const version = sessionVersion(), day = todayKey();
  try {
    const course = await api.attendanceCourse(c.code);
    if (isCurrentSession(version) && todayKey() === day) classWatch.school[c.code] = { course, at: Date.now() };
  } catch (e) { handleAuthError(e); }
}
export function afterSubmit(submission: AttendanceSubmission) {
  const receipt = submission.receipt;
  if (!receipt || !validReceipt(receipt)) return false;
  classWatch.receipts = [...classWatch.receipts.filter((entry) => entry.receipt.lecture.key !== receipt.lecture.key && validReceipt(entry.receipt)), { receipt, synced: submission.synced }];
  persist();
  classWatch.shareError = submission.synced ? '' : '출석은 확인했지만 다른 기기에 기록을 공유하지 못했어요. 서버에 연결되면 다시 시도할게요.';
  void syncReceipts();
  const version = sessionVersion(), day = todayKey(), ref = lectureRef(receipt.lecture, receipt.confirmedAt);
  if (ref.code) setTimeout(() => { if (isCurrentSession(version) && todayKey() === day) void checkSchool(ref); }, 1500);
  return true;
}
function resetClassWatch() {
  classWatch.receipts = loadSaved(); classWatch.school = {}; classWatch.seenOpen = []; classWatch.shareError = '';
  classWatch.day = todayKey(); classWatch.polling = false; classWatch.current = null; classWatch.nextAt = 0;
  statusAt = 0; sharedAt = 0; pendingCheck = null; pendingSync = null;
}
onSessionChange(resetClassWatch);
