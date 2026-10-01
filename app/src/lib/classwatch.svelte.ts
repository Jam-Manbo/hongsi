import { api } from './api';
import { dayKey, todayKey } from './format';
import { attendance, handleAuthError, lectures, timetable } from './store.svelte';
import { isCurrentSession, onSessionChange, readUserData, sessionVersion, writeUserData } from './session';
import type { ActiveLecture, AttendanceCourse, ClassSlot, MarkKind } from './types';

export const WATCH_BEFORE = 3 * 60_000;
export const WATCH_AFTER = 10 * 60_000;
export const POLL_MS = 5_000;
const STATUS_MS = 60_000;
const MIN_SPIN_MS = 700;
const KST = 9 * 3600_000;
const STORE = 'attended-v2';

export type TodayClass = ClassSlot & { at: number };
export type Mark = { label: string; kind: MarkKind; source: 'school' | 'app' | 'list' };
type CourseRef = { code: string | null; name: string; at?: number; lectureKey?: string };
type SavedMark = Mark & { ref: CourseRef; recordedAt: number };

export const SOURCE_TEXT: Record<Mark['source'], string> = {
  school: '학교 출결부에서 확인',
  app: '학교가 출석 처리 성공을 응답했어요',
  list: '',
};

export const sourceSuffix = (m: Mark) => (SOURCE_TEXT[m.source] ? ` · ${SOURCE_TEXT[m.source]}` : '');

const LABEL: Partial<Record<MarkKind, string>> = { present: '출석', late: '지각', excused: '공결', absent: '결석' };
const ATTENDED: MarkKind[] = ['present', 'late', 'excused'];

const norm = (s: string) => s.replace(/\(\*\)|\s/g, '');
const keyOf = (c: CourseRef) => `${c.at ? dayKey(c.at) : todayKey()}|${c.code || norm(c.name)}|${c.at ?? c.lectureKey ?? 'course'}`;

function loadSaved(): Record<string, SavedMark> {
  try {
    const all = readUserData<Record<string, SavedMark>>(STORE, {});
    const today = todayKey();
    return Object.fromEntries(Object.entries(all).filter(([k, m]) => k.startsWith(`${today}|`) && m?.ref
      && ['school', 'app', 'list'].includes(m.source) && Number.isFinite(m.recordedAt)));
  } catch {
    return {};
  }
}

export const classWatch = $state({
  current: null as TodayClass | null,
  nextAt: 0,
  polling: false,
  now: Date.now(),
  marks: loadSaved(),
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

export function todayMark(course: AttendanceCourse, now = Date.now()): Mark | null {
  if (!course.published) return null;
  const k = new Date(now + KST);
  const key = `${k.getUTCMonth() + 1}/${k.getUTCDate()}`;
  const marks = course.weeks.flatMap((w) => w.sessions).filter((s) => s.date === key);
  const kind = marks[0]?.kind;
  return kind && LABEL[kind] && marks.every((s) => s.kind === kind)
    ? { label: LABEL[kind]!, kind, source: 'school' } : null;
}

type AttendedMark = Mark & { kind: 'present' | 'late' | 'excused' };
type ConfirmedMark = AttendedMark & { source: 'school' | 'app' };
export const isAttended = (m: Mark | null): m is AttendedMark => !!m && ATTENDED.includes(m.kind);
export const isConfirmed = (m: Mark | null): m is ConfirmedMark => isAttended(m) && (m.source === 'school' || m.source === 'app');
export const markTitle = (m: Mark) => m.kind === 'present' ? '출석 완료' : `${m.label} 처리됨`;

function schoolMark(c: CourseRef, course: AttendanceCourse): Mark | null {
  const classes = todayClasses(timetable.data?.slots ?? []).filter((s) => sameCourse(s, c));
  if (classes.length > 1 || (classes.length && c.at === undefined)) return null;
  const k = new Date(Date.now() + KST);
  const date = `${k.getUTCMonth() + 1}/${k.getUTCDate()}`;
  const entries = course.weeks.flatMap((w) => w.sessions).filter((s) => s.date === date);
  if (classes[0] && entries.length < classes[0].periods.length) return null;
  return todayMark(course);
}

export function markFor(c: CourseRef): Mark | null {
  const saved = classWatch.marks[keyOf(c)];
  const course = c.code ? attendance.data?.find((a) => a.code === c.code) : undefined;
  const school = course ? schoolMark(c, course) : null;
  if (school && (!saved || attendance.at >= saved.recordedAt)) return school;
  return saved ?? school;
}

function lectureRef(l: ActiveLecture): CourseRef {
  const classes = todaySessions(timetable.data?.slots ?? []).filter((c) => sameCourse(l, c));
  const time = /(?:^|\D)(\d{1,2}):(\d{2})/.exec(l.time);
  const start = time ? `${time[1].padStart(2, '0')}:${time[2]}` : null;
  const cls = start ? classes.find((c) => c.start === start)
    : classes.find((c) => inWindow(c)) ?? (classes.length === 1 ? classes[0] : undefined);
  return cls ?? { code: l.code ?? null, name: l.name, lectureKey: l.key };
}

export function lectureMark(l: ActiveLecture): Mark | null {
  const m = markFor(lectureRef(l));
  return m?.source === 'list' ? null : m;
}

function remember(c: CourseRef, mark: Mark) {
  classWatch.marks[keyOf(c)] = { ...mark, ref: { code: c.code, name: c.name, at: c.at, lectureKey: c.lectureKey }, recordedAt: Date.now() };
  persistMarks();
}

function persistMarks() {
  try {
    writeUserData(STORE, $state.snapshot(classWatch.marks));
  } catch {
  }
}

let users = 0;
let timer: ReturnType<typeof setInterval> | undefined;
let statusAt = 0;
let seenOpen = false;
let observedAt = 0;
let pendingCheck: Promise<void> | null = null;

export function useClassWatch(): () => void {
  users += 1;
  if (users === 1) {
    timer = setInterval(tick, 1000);
    document.addEventListener('visibilitychange', onVisibility);
  }
  void checkNow();
  return () => {
    users -= 1;
    if (users === 0) {
      clearInterval(timer);
      document.removeEventListener('visibilitychange', onVisibility);
    }
  };
}

export function checkNow(): Promise<void> {
  if (pendingCheck) return pendingCheck;
  tick(false);
  classWatch.nextAt = Date.now() + POLL_MS;
  const request = poll().finally(() => { if (pendingCheck === request) pendingCheck = null; });
  pendingCheck = request;
  return request;
}

function onVisibility() {
  if (document.visibilityState === 'visible') tick();
}

function tick(automatic = true) {
  const now = Date.now();
  classWatch.now = now;
  const cur = todaySessions(timetable.data?.slots ?? [], now).find((c) => inWindow(c, now)) ?? null;
  const prev = classWatch.current;
  if (!sameClass(cur, prev)) {
    classWatch.current = cur;
    classWatch.nextAt = Math.max(now, lectures.at + POLL_MS);
    statusAt = 0;
    seenOpen = false;
  }
  observeActiveList();
  const mark = cur ? markFor(cur) : null;
  if (!automatic || !cur || document.visibilityState !== 'visible' || isAttended(mark)) return;
  if (now >= classWatch.nextAt && !classWatch.polling) {
    void checkNow();
  }
  if (cur.code && (statusAt === 0 || now - statusAt >= STATUS_MS)) {
    statusAt = now;
    void checkSchool(cur);
  }
}

async function poll() {
  const version = sessionVersion();
  classWatch.polling = true;
  const started = Date.now();
  await lectures.load(true);
  if (!isCurrentSession(version)) return;
  tick(false);
  const rest = MIN_SPIN_MS - (Date.now() - started);
  if (rest > 0) await new Promise((r) => setTimeout(r, rest));
  if (isCurrentSession(version)) classWatch.polling = false;
}

export function observeActiveList() {
  if (lectures.error) return;
  if (!lectures.data || lectures.loading || lectures.at <= observedAt || Date.now() - lectures.at > POLL_MS * 2) return;
  observedAt = lectures.at;
  let changed = false;
  for (const [key, mark] of Object.entries(classWatch.marks)) {
    if (mark.source === 'list' && lectures.at >= mark.recordedAt
      && lectures.data.items.some((l) => keyOf(lectureRef(l)) === key)) {
      delete classWatch.marks[key];
      changed = true;
    }
  }
  if (changed) persistMarks();
  const cur = classWatch.current;
  if (cur && inWindow(cur)) judge(cur);
}

function judge(cur: TodayClass) {
  const listed = (lectures.data?.items ?? []).some((l) => keyOf(lectureRef(l)) === keyOf(cur));
  if (listed) {
    seenOpen = true;
    return;
  }
  if (!seenOpen) return;
  if (isAttended(markFor(cur))) return;
  remember(cur, { label: '출석', kind: 'present', source: 'list' });
}

async function checkSchool(c: CourseRef) {
  if (!c.code) return;
  const version = sessionVersion();
  const day = todayKey();
  const before = classWatch.marks[keyOf(c)];
  try {
    const course = await api.attendanceCourse(c.code);
    if (!isCurrentSession(version) || todayKey() !== day || classWatch.marks[keyOf(c)] !== before) return;
    const mark = schoolMark(c, course);
    if (mark) remember(c, mark);
  } catch (e) {
    handleAuthError(e);
  }
}

const SUCCESS = /(?:출석|출결|지각)(?:이|가|은|는|으로)?\s*(?:정상적으로\s*)?(?:처리(?:가)?\s*)?(?:(?:완료|성공)(?:되었습니다|됐습니다|했습니다|하였습니다)?|되었습니다|됐습니다|하였습니다|했습니다|하셨습니다)[.!。\s]*$/;
const FAILURE = /실패|오류|에러|틀렸|틀립|잘못|아닙|않|없|만료|초과|불가|벗어|못했|못하|되지|되기|완료하려|완료하기|예정|가능|미완료/;

export const submissionConfirmed = (message: string) => SUCCESS.test(message) && !FAILURE.test(message);

export function afterSubmit(l: ActiveLecture, message: string) {
  const version = sessionVersion();
  const day = todayKey();
  const ref = lectureRef(l);
  const confirmed = submissionConfirmed(message);
  if (confirmed) {
    const late = message.includes('지각');
    remember(ref, { label: late ? '지각' : '출석', kind: late ? 'late' : 'present', source: 'app' });
  }
  if (ref.code) setTimeout(() => { if (isCurrentSession(version) && todayKey() === day) void checkSchool(ref); }, 1500);
  return confirmed;
}

export function resetClassWatch() {
  classWatch.marks = loadSaved();
  classWatch.polling = false;
  classWatch.current = null;
  classWatch.nextAt = 0;
  statusAt = 0;
  seenOpen = false;
  observedAt = 0;
  pendingCheck = null;
}

onSessionChange(resetClassWatch);
