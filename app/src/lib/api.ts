import { isApp } from './env';
import { reportSchool, reportServer } from './net.svelte';
import { inSession } from './session';
import type {
  ActiveLectures,
  AttendanceCourse,
  BoardArticle,
  CalendarData,
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

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

export { isApp };

type Envelope = { status: number; body: unknown; server?: boolean | null };

const SCHOOL_PATH = /^\/api\/(calendar(\?|$)|calendar\/items\/[^/]+\/verify|attendance\/|timetable|notifications|assign\/|modules\/|board\/)/;

function errorOf(data: unknown) {
  return (data as { error?: { code?: string; message?: string } } | null)?.error;
}

function note(path: string, status: number, data: unknown, server?: boolean | null) {
  if (isApp) {
    if (typeof server === 'boolean') reportServer(server);
  } else {
    reportServer(!(data === null && status >= 502 && status <= 504));
  }
  if (!SCHOOL_PATH.test(path)) return;
  const code = errorOf(data)?.code ?? '';
  if (status < 400) reportSchool(true);
  else if (code === 'school_unreachable' || code === 'school_error') reportSchool(false);
}

export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  return inSession((check) => sendRequest<T>(method, path, body, check));
}

async function sendRequest<T>(method: string, path: string, body: unknown, check: () => void): Promise<T> {
  let status = 0;
  let data: unknown = null;
  if (isApp) {
    const { invoke } = await import('@tauri-apps/api/core');
    check();
    let res: Envelope;
    try {
      res = await invoke<Envelope>('api', { method, path, body: body ?? null });
    } catch (e) {
      throw new ApiError(0, 'native', String(e));
    }
    status = res.status;
    data = res.body;
    check();
    note(path, status, data, res.server);
  } else {
    let res: Response;
    try {
      res = await fetch(path, {
        method,
        credentials: 'same-origin',
        headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      check();
      reportServer(false);
      throw new ApiError(0, 'offline', '서버에 연결하지 못했어요');
    }
    status = res.status;
    data = await res.json().catch(() => null);
    check();
    note(path, status, data);
  }
  if (status >= 400) {
    const err = errorOf(data);
    throw new ApiError(status, err?.code ?? 'error', err?.message ?? `요청을 처리하지 못했어요 (${status})`);
  }
  if (data === null || typeof data !== 'object') {
    throw new ApiError(status, 'invalid_response', '서버 응답을 확인하지 못했어요. 잠시 뒤 다시 시도해 주세요.');
  }
  return data as T;
}

export const api = {
  login: (id: string, password: string, remember: boolean) =>
    request<{ profile: Profile }>('POST', '/api/auth/login', { id, password, remember }),
  logout: () => request<{ ok: boolean }>('POST', '/api/auth/logout'),
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
    request<{ message: string }>('POST', '/api/attendance/submit', { lectureKey, code, latitude, longitude }),
  attendanceStatus: () => request<AttendanceCourse[]>('GET', '/api/attendance/status'),
  attendanceCourse: (code: string) =>
    request<AttendanceCourse>('GET', `/api/attendance/course?code=${encodeURIComponent(code)}`),
  timetable: (refresh = false) => request<Timetable>('GET', `/api/timetable${refresh ? '?refresh=1' : ''}`),

  calendar: (refresh = false) => request<CalendarData>('GET', `/api/calendar${refresh ? '?refresh=1' : ''}`),
  setDone: (key: string, done: boolean | null) =>
    request<{ key: string; done: boolean }>('PUT', `/api/calendar/items/${encodeURIComponent(key)}/done`, { done }),
  setAlert: (key: string, on: boolean) =>
    request<{ key: string; on: boolean }>('PUT', `/api/calendar/items/${encodeURIComponent(key)}/alert`, { on }),

  meals: () => request<MealDay[]>('GET', '/api/meals'),

  submission: (cmid: number) => request<SubmissionView>('GET', `/api/assign/${cmid}/submission`),
  async submit(cmid: number, keep: string[], files: File[], lateConfirmed: boolean, acceptStatement: boolean): Promise<SubmissionView> {
    return inSession(async (check) => {
      let status = 0;
      let data: unknown = null;
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
          throw new ApiError(0, 'offline', '서버에 연결하지 못했어요');
        }
        status = res.status;
        data = await res.json().catch(() => null);
        check();
        note(`/api/assign/${cmid}/submission`, status, data);
      }
      if (status >= 400) {
        const err = errorOf(data);
        throw new ApiError(status, err?.code ?? 'error', err?.message ?? `제출하지 못했어요 (${status})`);
      }
      return data as SubmissionView;
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
