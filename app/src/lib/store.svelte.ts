import { ApiError, api, isApp, onSessionRevoked, onSchoolAuthRequired } from './api';
import { normalizeCalendar } from './calendar-data';
import { errorText, onReconnect, troubleOf, type Trouble } from './net.svelte';
import { clearUserData, isStaleSession, onSessionChange, readUserData, setSessionUser, writeUserData } from './session';
import type {
  ActiveLectures,
  AttendanceCourse,
  AttendanceReceipt,
  CalendarData,
  ClassNotification,
  MealDay,
  Profile,
  SeatSession,
  SeatsData,
  SemesterDisplay,
  Timetable,
  Todo,
} from './types';

const PREFIX = 'hc:';

export const app = $state({
  profile: null as Profile | null,
  account: null as string | null,
  booting: true,
  notice: '',
  remembered: false,
  loggingOut: false,
});

type Cached<T> = { data: T; at: number };

function readCache<T>(key: string): Cached<T> | null {
  return readUserData<Cached<T> | null>(key, null);
}

function writeCache<T>(key: string, data: T, at: number) {
  writeUserData(key, { data, at });
}

let logoutPending: Promise<void> | null = null;
const logoutPreparations = new Set<() => Promise<void>>();

export const waitForLogout = () => logoutPending ?? Promise.resolve();
export function onBeforeLogout(prepare: () => Promise<void>) {
  logoutPreparations.add(prepare);
  return () => logoutPreparations.delete(prepare);
}

export function startSession(profile: Profile, remembered: boolean, id = profile.studentId) {
  app.account = id.trim().toUpperCase() || null;
  setSessionUser(app.account);
  app.notice = '';
  app.remembered = remembered;
  app.profile = profile;
}

export function endSession() {
  clearUserData();
  app.profile = null;
  app.account = null;
  app.remembered = false;
  setSessionUser(null);
}

export function logoutSession(scope: 'device' | 'all' = 'device') {
  if (logoutPending) return logoutPending;
  app.loggingOut = true;
  let revoked = false;
  const pending = (async () => {
    await Promise.all([...logoutPreparations].map((prepare) => prepare()));
    const result = await (scope === 'all' ? api.logoutAll() : api.logout());
    if (result.ok !== true) throw new ApiError(200, 'logout_incomplete', '로그인 정보 삭제를 확인하지 못했어요. 다시 시도해 주세요.');
    endSession();
  })().catch((e) => {
    revoked = e instanceof ApiError && e.status === 401 && e.code === 'session_revoked';
    throw e;
  }).finally(() => {
    app.loggingOut = false;
    if (logoutPending === pending) logoutPending = null;
    if (revoked) handleSessionRevoked();
  });
  logoutPending = pending;
  return pending;
}

function handleSessionRevoked() {
  if (app.loggingOut) return;
  if (app.profile || app.account) endSession();
  app.notice = '다시 로그인해 주세요.';
}
function handleSchoolSessionExpired() {
  if (!app.profile || app.loggingOut) return;
  void logoutSession().catch(() => {
    if (app.profile || app.account) endSession();
  }).then(() => {
    app.notice = '로그인이 만료됐어요.';
  });
}

onSessionRevoked(handleSessionRevoked);
onSchoolAuthRequired(handleSchoolSessionExpired);

export function handleAuthError(e: unknown): boolean {
  if (isStaleSession(e)) return true;
  if (e instanceof ApiError && ['session_expired', 'classroom_token_expired', 'school_reauth_required'].includes(e.code)) {
    handleSchoolSessionExpired();
    return true;
  }
  if (e instanceof ApiError && e.status === 401 && e.code !== 'login_rejected') {
    if (e.code === 'session_revoked') { handleSessionRevoked(); return true; }
    if (app.loggingOut) return true;
    endSession();
    app.notice = '';
    return true;
  }
  return false;
}

export class Resource<T> {
  data = $state<T | null>(null);
  at = $state(0);
  loading = $state(false);
  error = $state<string | null>(null);
  trouble = $state<Trouble | 'other' | null>(null);
  private lastStart = 0;
  private revision = 0;
  private generation = 0;
  private mutations = 0;
  private pending: Promise<void> | null = null;
  private refreshAfterMutation = false;

  constructor(
    private key: string,
    private fetcher: (force: boolean) => Promise<T>,
    private maxAgeMs: number,
    private normalize: (data: T) => T = (data) => data,
  ) {}

  restore() {
    this.reset();
    const cached = readCache<T>(this.key);
    if (cached) {
      this.data = this.normalize(cached.data);
      this.at = cached.at;
    }
  }

  setKey(key: string) {
    if (this.key === key) return;
    this.key = key;
    this.restore();
  }

  get stale() {
    return Date.now() - this.at > this.maxAgeMs;
  }

  load(force = false): Promise<void> {
    if (app.loggingOut || this.mutations) return Promise.resolve();
    if (this.pending) return this.pending;
    if (!force && this.data !== null && !this.stale) return Promise.resolve();
    if (Date.now() - this.lastStart < 3000) return Promise.resolve();
    this.lastStart = Date.now();
    const revision = this.revision;
    this.loading = true;
    const pending = this.fetch(force, revision);
    this.pending = pending;
    void pending.finally(() => { if (this.pending === pending) this.pending = null; });
    return pending;
  }

  refresh(): Promise<void> {
    if (this.mutations) {
      this.refreshAfterMutation = true;
      return Promise.resolve();
    }
    this.revision += 1;
    this.pending = null;
    this.lastStart = 0;
    return this.load(true);
  }

  private async fetch(force: boolean, revision: number) {
    try {
      const data = await this.fetcher(force);
      if (revision !== this.revision) return;
      this.set(data);
      this.error = null;
      this.trouble = null;
    } catch (e) {
      if (revision !== this.revision) return;
      if (!handleAuthError(e)) {
        this.trouble = (e instanceof ApiError ? troubleOf(e.status, e.code) : null) ?? 'other';
        this.error = errorText(e, '불러오지 못했어요.');
      }
    } finally {
      if (revision === this.revision) this.loading = false;
    }
  }

  set(data: T, at = Date.now()) {
    data = this.normalize(data);
    this.data = data;
    this.at = at;
    writeCache(this.key, data, this.at);
  }

  beginMutation() {
    const generation = this.generation;
    this.mutations += 1;
    this.revision += 1;
    this.pending = null;
    this.loading = false;
    let finished = false;
    return () => {
      if (finished || generation !== this.generation) return;
      finished = true;
      this.mutations -= 1;
      this.revision += 1;
      this.lastStart = 0;
      if (!this.mutations && this.refreshAfterMutation) {
        this.refreshAfterMutation = false;
        void this.refresh();
      }
    };
  }

  reset() {
    this.generation += 1;
    this.mutations = 0;
    this.refreshAfterMutation = false;
    this.revision += 1;
    this.pending = null;
    this.lastStart = 0;
    this.loading = false;
    this.data = null;
    this.at = 0;
    this.error = null;
    this.trouble = null;
  }
}

const MIN = 60_000;
let calendarSemesterDisplay: SemesterDisplay = 'current';
export const calendar = new Resource<CalendarData>('calendar-current', async (force) => {
  const data = await api.calendar(force, calendarSemesterDisplay);
  await todos.refresh();
  return data;
}, 5 * MIN, normalizeCalendar);

export function setCalendarSemesterDisplay(display: SemesterDisplay) {
  if (calendarSemesterDisplay === display) return;
  calendarSemesterDisplay = display;
  calendar.setKey(`calendar-${display}`);
  if (app.profile) void calendar.load();
}
export const seats = new Resource<SeatsData>('seats', () => api.seats(), MIN);
export const seatSession = new Resource<{ session: SeatSession | null }>('seat-session', () => api.seatSession(), MIN);
export const meals = new Resource<MealDay[]>('meals', () => api.meals(), 30 * MIN);
export const lectures = new Resource<ActiveLectures>('lectures', () => api.activeLectures(), MIN / 2);
export const attendanceReceipts = new Resource<AttendanceReceipt[]>('attendance-receipts', () => api.attendanceReceipts(), 5_000);
export const attendance = new Resource<AttendanceCourse[]>('attendance', () => api.attendanceStatus(), 10 * MIN);
export const timetable = new Resource<Timetable>('timetable', (force) => api.timetable(force), 360 * MIN);

export const todos = new Resource<Todo[]>('todos', () => api.todos(), 5 * MIN);
export const notices = new Resource<ClassNotification[]>('notices', () => api.notifications(), 5 * MIN);

export const allResources = [calendar, seats, seatSession, meals, lectures, attendance, attendanceReceipts, timetable, todos, notices];
onSessionChange(() => allResources.forEach((r) => r.restore()));

onReconnect((what) => {
  if (!app.profile || app.loggingOut) return;
  const jobs = allResources.filter((r) => r.error !== null && (what === 'server' || r.trouble === 'school'));
  if (what === 'server' && isApp && calendar.data && !jobs.includes(calendar)) jobs.push(calendar);
  return Promise.all(jobs.map((r) => r.load(true)));
});

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
