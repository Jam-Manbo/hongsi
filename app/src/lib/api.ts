import { isApp } from './env';
import { ApiError, connectionError, errorOf, invalidResponse, reportApiFailure, responseError, responseFormat, submissionError, type ResponseFormat } from './api-error';
export { ApiError } from './api-error';
import { ReadTimeoutError, withReadTimeout } from './http';
import { reportSchool, reportServer } from './net.svelte';
import { inSession, sessionUser, sessionVersion } from './session';
import type {
  AccountPreferences,
  AccountPreferenceChanges,
  ActiveLectures,
  AttendanceCourse,
  AttendanceReceipt,
  AttendanceSubmission,
  BoardArticle,
  CalendarData,
  CalendarState,
  ClassNotification,
  FileSource,
  MealDay,
  ModuleContents,
  Profile,
  RecentSeat,
  SeatPeriod,
  SeatSession,
  SeatsData,
  SubmissionView,
  Timetable,
  Todo,
  TodoInput,
} from './types';

export { isApp };

type Envelope = { status: number; body: unknown; server?: boolean | null };
const sessionRevokedListeners = new Set<() => void>();

export function onSessionRevoked(listener: () => void) {
  sessionRevokedListeners.add(listener);
  return () => sessionRevokedListeners.delete(listener);
}

const SCHOOL_PATH = /^\/api\/(calendar(\?|$)|calendar\/items\/[^/]+\/verify|attendance\/|timetable|notifications|assign\/|modules\/|board\/)/;

function note(path: string, status: number, data: unknown, server?: boolean | null) {
  if (isApp) {
    if (typeof server === 'boolean') reportServer(server);
  } else {
    reportServer(!(data === null && status >= 502 && status <= 504));
  }
  if (path.startsWith('/api/attendance/receipts') || !SCHOOL_PATH.test(path)) return;
  const code = errorOf(data)?.code ?? '';
  if (status < 400) reportSchool(true);
  else if (code === 'school_unreachable' || code === 'school_error') reportSchool(false);
}

const schoolAuthListeners = new Set<() => void>();
const recoveryJobs = new Map<string, Promise<unknown>>();

export function onSchoolAuthRequired(listener: () => void) {
  schoolAuthListeners.add(listener);
  return () => schoolAuthListeners.delete(listener);
}

export function schoolAuthRequired() {
  schoolAuthListeners.forEach((listener) => listener());
}

const schoolAuthError = (e: unknown): e is ApiError => e instanceof ApiError && e.status === 401
  && (e.code === 'session_expired' || e.code === 'classroom_token_expired');

async function recoverSchoolAuth(e: ApiError, path: string, check: () => void) {
  const kind = path.startsWith('/api/attendance/') ? 'attendance' : path.startsWith('/api/timetable') ? 'timetable'
    : e.code === 'classroom_token_expired' ? 'classroom_token' : 'classroom';
  const key = `${sessionVersion()}:${kind}`;
  let pending = recoveryJobs.get(key);
  if (!pending) {
    pending = sendRequest('POST', '/api/auth/recover', { kind }, check);
    recoveryJobs.set(key, pending);
    void pending.finally(() => { if (recoveryJobs.get(key) === pending) recoveryJobs.delete(key); }).catch(() => {});
  }
  await pending;
  check();
}

async function withSchoolRecovery<T>(method: string, path: string, body: unknown, check: () => void): Promise<T> {
  try {
    return await sendRequest<T>(method, path, body, check);
  } catch (e) {
    if (isApp || !schoolAuthError(e) || path.startsWith('/api/auth/')) throw e;
    await recoverSchoolAuth(e, path, check);
    if (method !== 'GET') throw new ApiError(409, 'school_retry_required', '다시 시도해 주세요.');
    try {
      return await sendRequest<T>(method, path, body, check);
    } catch (retryError) {
      if (schoolAuthError(retryError)) throw new ApiError(409, 'school_reauth_required', '학교에 다시 로그인해 주세요.');
      throw retryError;
    }
  }
}

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  try {
    return await inSession((check) => withSchoolRecovery<T>(method, path, body, check));
  } catch (e) {
    if (!isApp && e instanceof ApiError && e.code === 'school_reauth_required') schoolAuthRequired();
    if (e instanceof ApiError && e.status === 401 && e.code === 'session_revoked') {
      sessionRevokedListeners.forEach((listener) => listener());
    }
    throw method === 'POST' && path === '/api/attendance/submit' ? submissionError(e, 'attendance') : e;
  }
}

async function sendRequest<T>(method: string, path: string, body: unknown, check: () => void): Promise<T> {
  let status = 0;
  let data: unknown = null;
  let format: ResponseFormat = isApp ? 'native' : 'none';
  if (isApp) {
    const { invoke } = await import('@tauri-apps/api/core');
    check();
    let res: Envelope;
    try {
      res = await invoke<Envelope>('api', { method, path, body: body ?? null });
    } catch {
      reportApiFailure(0, 'native', { method, path, format });
      throw new ApiError(0, 'native', '앱에서 요청을 처리하지 못했어요.');
    }
    status = res.status;
    data = res.body;
    check();
    note(path, status, data, res.server);
  } else {
    const fetchResponse = async (signal?: AbortSignal) => {
      const res = await fetch(path, {
        method,
        credentials: 'same-origin',
        headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
        signal,
      });
      format = responseFormat(res.headers.get('content-type'));
      return { status: res.status, data: await res.json().catch(() => null) };
    };
    try {
      ({ status, data } = method === 'GET' || path === '/api/auth/recover'
        ? await withReadTimeout(fetchResponse)
        : await fetchResponse());
    } catch (e) {
      check();
      if (e instanceof ReadTimeoutError) {
        throw connectionError(true, { method, path, format });
      }
      reportServer(false);
      throw connectionError(false, { method, path, format });
    }
    check();
    note(path, status, data);
  }
  if (status >= 400) {
    throw responseError(status, data, { method, path, format });
  }
  if (data === null || typeof data !== 'object') {
    throw invalidResponse(status, { method, path, format });
  }
  return data as T;
}

export const api = {
  preferences: () => request<AccountPreferences>('GET', '/api/preferences'),
  updatePreferences: (changes: AccountPreferenceChanges) => request<AccountPreferences>('PATCH', '/api/preferences', changes),
  login: (id: string, password: string, remember: boolean) =>
    request<{ profile: Profile }>('POST', '/api/auth/login', { id, password, remember }),
  logout: () => request<{ ok: boolean }>('POST', '/api/auth/logout'),
  logoutAll: () => request<{ ok: boolean }>('POST', '/api/auth/logout-all'),
  me: () => request<{ profile: Profile; remembered: boolean }>('GET', '/api/me'),
  verifyItem: (key: string, courseId: number) =>
    request<{ key: string; status: string; finished: boolean }>(
      'POST',
      `/api/calendar/items/${encodeURIComponent(key)}/verify?course=${courseId}`,
    ),
  fileUrl(src: FileSource, inline = false) {
    const base =
      src.kind === 'assign'
        ? `/api/files/${src.cmid}/${src.index}`
        : src.kind === 'module'
          ? `/api/modules/${src.cmid}/files/${src.index}`
          : `/api/board/${src.cmid}/${src.bwid}/files/${src.index}`;
    return inline ? `${base}?inline=1` : base;
  },
  module: (cmid: number) => request<ModuleContents>('GET', `/api/modules/${cmid}`),
  article: (cmid: number, bwid: number) => request<BoardArticle>('GET', `/api/board/${cmid}/${bwid}`),

  activeLectures: () => request<ActiveLectures>('GET', '/api/attendance/active'),
  submitAttendance: (lectureKey: string, code: string, latitude: number, longitude: number) =>
    request<AttendanceSubmission>('POST', '/api/attendance/submit', { lectureKey, code, latitude, longitude }),
  attendanceReceipts: () => request<AttendanceReceipt[]>('GET', '/api/attendance/receipts'),
  shareAttendanceReceipt: (receipt: AttendanceReceipt) => request<{ ok: boolean }>('PUT', '/api/attendance/receipts', { receipt, account: sessionUser() }),
  attendanceStatus: () => request<AttendanceCourse[]>('GET', '/api/attendance/status'),
  attendanceCourse: (code: string) =>
    request<AttendanceCourse>('GET', `/api/attendance/course?code=${encodeURIComponent(code)}`),
  timetable: (refresh = false) => request<Timetable>('GET', `/api/timetable${refresh ? '?refresh=1' : ''}`),

  calendar: (refresh = false) => request<CalendarData>('GET', `/api/calendar${refresh ? '?refresh=1' : ''}`),
  calendarState: () => request<CalendarState>('GET', '/api/calendar/state'),
  setDone: (key: string, done: boolean | null) =>
    request<{ key: string; done: boolean }>('PUT', `/api/calendar/items/${encodeURIComponent(key)}/done`, { done }),
  setAlertLeads: (key: string, leads: number[] | null) =>
    request<{ key: string; leads: number[] | null }>('PUT', `/api/calendar/items/${encodeURIComponent(key)}/alert-leads`, { leads }),
  setAlert: (key: string, on: boolean) =>
    request<{ key: string; on: boolean }>('PUT', `/api/calendar/items/${encodeURIComponent(key)}/alert`, { on }),

  meals: () => request<MealDay[]>('GET', '/api/meals'),

  submission: (cmid: number) => request<SubmissionView>('GET', `/api/assign/${cmid}/submission`),
  async submit(cmid: number, keep: string[], files: File[], lateConfirmed: boolean, acceptStatement: boolean): Promise<SubmissionView> {
    return inSession(async (check) => {
      let status = 0;
      let data: unknown = null;
      let format: ResponseFormat = isApp ? 'native' : 'none';
      const path = `/api/assign/${cmid}/submission`, method = 'POST';
      try {
        if (isApp) {
          const encoded = await Promise.all(files.map(async (f) => ({ name: f.name, data: await toBase64(f) })));
          check();
          const res = await native.call<Envelope>('submit_assignment', { cmid, keep, files: encoded, lateConfirmed, acceptStatement });
          status = res.status;
          data = res.body;
          note(`/api/assign/${cmid}/submission`, status, data, res.server);
        } else {
          const form = new FormData();
          keep.forEach((k) => form.append('keep', k));
          form.append('lateConfirmed', lateConfirmed ? '1' : '0');
          form.append('acceptStatement', acceptStatement ? '1' : '0');
          files.forEach((f) => form.append('file', f, f.name));
          let res: Response;
          try {
            res = await fetch(`/api/assign/${cmid}/submission`, { method: 'POST', body: form, credentials: 'same-origin' });
          } catch {
            check();
            reportServer(false);
            throw connectionError(false, { method, path, format });
          }
          status = res.status;
          format = responseFormat(res.headers.get('content-type'));
          data = await res.json().catch(() => null);
          check();
          note(`/api/assign/${cmid}/submission`, status, data);
        }
        if (status >= 400) {
          const error = responseError(status, data, { method, path, format });
          if (!isApp && schoolAuthError(error)) {
            try {
              await recoverSchoolAuth(error, `/api/assign/${cmid}/submission`, check);
            } catch (recoveryError) {
              if (recoveryError instanceof ApiError && recoveryError.code === 'school_reauth_required') schoolAuthRequired();
              throw recoveryError;
            }
            throw new ApiError(409, 'school_retry_required', '학교에 다시 연결됐어요. 제출 상태를 확인한 뒤 다시 시도해 주세요.');
          }
          throw error;
        }
        if (!data || typeof data !== 'object' || Array.isArray(data)) throw invalidResponse(status, { method, path, format });
        return data as SubmissionView;
      } catch (e) { throw submissionError(e, 'assignment'); }
    });
  },

  notifications: () => request<ClassNotification[]>('GET', '/api/notifications'),
  noticesSeen: () => request<{ urls: string[] }>('GET', '/api/notices/seen'),
  markNoticesSeen: (urls: string[]) => request<{ ok: boolean }>('POST', '/api/notices/seen', { urls }),

  todos: () => request<Todo[]>('GET', '/api/todos'),
  createTodo: (t: TodoInput) => request<Todo>('POST', '/api/todos', t),
  updateTodo: (id: number, t: TodoInput) => request<Todo>('PUT', `/api/todos/${id}`, t),
  setTodoDone: (id: number, done: boolean) => request<Todo>('POST', `/api/todos/${id}/done`, { done }),
  deleteTodo: (id: number) => request<{ ok: boolean }>('DELETE', `/api/todos/${id}`),

  seats: () => request<SeatsData>('GET', '/api/seats'),
  recentSeats: (building: string, room: number) =>
    request<RecentSeat[]>('GET', `/api/seats/recent?building=${encodeURIComponent(building)}&room=${room}&minutes=20`),
  seatSession: () => request<{ session: SeatSession | null }>('GET', '/api/seats/session'),
  checkIn: (building: string, roomNo: number, seatNo: number, period: SeatPeriod) =>
    request<{ session: SeatSession }>('POST', '/api/seats/session', { building, roomNo, seatNo, period }),
  extendSeat: () => request<{ session: SeatSession }>('POST', '/api/seats/session/extend'),
  adjustSeat: (startedAt: number, period: SeatPeriod) =>
    request<{ session: SeatSession }>('PATCH', '/api/seats/session', { startedAt, period }),
  checkOut: () => request<{ session: null }>('POST', '/api/seats/session/end'),
};

async function toBase64(file: File): Promise<string> {
  const buf = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  for (let i = 0; i < buf.length; i += 0x8000) binary += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  return btoa(binary);
}

export const native = {
  async call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    return inSession(async (check) => {
      const { invoke } = await import('@tauri-apps/api/core');
      check();
      try {
        return await invoke<T>(cmd, args);
      } catch (e) {
        throw new ApiError(0, 'native', String(e));
      }
    });
  },
  autoLoginEnabled: () => native.call<boolean>('auto_login_enabled'),
  download: (source: FileSource, name: string) => native.call<string>('download', { source, name }),
  openFile: (path: string) => native.call<void>('open_file', { path }),
  revealFile: (path: string) => native.call<void>('reveal_file', { path }),
  openUrl: (url: string) => native.call<void>('open_url', { url }),
  avatar: () => native.call<string | null>('avatar'),
};
