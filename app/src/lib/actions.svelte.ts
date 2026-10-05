import { api, isApp, native } from './api';
import { connectionError, responseError, responseFormat } from './api-error';
import { beginCalendarChange } from './calendar-sync.svelte';
import { errorText, reportServer, writeBlocked } from './net.svelte';
import { schoolFinished } from './colors';
import { calendar, handleAuthError, todos } from './store.svelte';
import { inSession, isCurrentSession, isStaleSession, onSessionChange, readUserData, sessionVersion, writeUserData } from './session';
import { toast, toastOnce } from './ui.svelte';
import type { CalendarItem, DownloadRecord, FileSource } from './types';

const saving = new Set<string>();
const savingAlerts = new Set<string>();
export const doneConfirmation = $state({ pending: null as { key: string; done: boolean } | null });

export function cancelDoneConfirmation() {
  doneConfirmation.pending = null;
}

export async function confirmDone() {
  const pending = doneConfirmation.pending;
  cancelDoneConfirmation();
  if (!pending) return;
  const item = calendar.data?.items.find((i) => i.key === pending.key);
  if (item) await saveDone(item, pending.done);
}

function update(key: string, patch: Partial<CalendarItem>) {
  if (!calendar.data) return;
  calendar.set({ ...calendar.data, items: calendar.data.items.map((i) => (i.key === key ? { ...i, ...patch } : i)) });
}

export async function toggleDone(item: CalendarItem) {
  if (writeBlocked() || saving.has(item.key) || doneConfirmation.pending) return;
  const current = calendar.data?.items.find((i) => i.key === item.key) ?? item;
  const done = !current.done;
  if (done !== schoolFinished(current)) {
    doneConfirmation.pending = { key: item.key, done };
    return;
  }
  await saveDone(current, done);
}

async function saveDone(item: CalendarItem, done: boolean) {
  if (writeBlocked() || saving.has(item.key)) return;
  const version = sessionVersion();
  const current = calendar.data?.items.find((i) => i.key === item.key) ?? item;
  if (done === current.done) return;
  const before = { done: current.done, doneOverride: current.doneOverride };
  const override = done === schoolFinished(current) ? null : done;
  const finishChange = beginCalendarChange();
  saving.add(item.key);
  update(item.key, { done, doneOverride: override });
  try {
    await api.setDone(item.key, override);
    if (isCurrentSession(version)) await todos.refresh();
  } catch (e) {
    if (isStaleSession(e)) return;
    update(item.key, before);
    if (!handleAuthError(e)) toastOnce(errorText(e, '저장하지 못했어요.'), 'error');
  } finally {
    if (isCurrentSession(version)) saving.delete(item.key);
    finishChange();
  }
}


async function saveItemAlert(item: CalendarItem, patch: Partial<CalendarItem>, save: () => Promise<unknown>) {
  if (writeBlocked() || savingAlerts.has(item.key)) return;
  const version = sessionVersion();
  const current = calendar.data?.items.find((i) => i.key === item.key) ?? item;
  const before = { alert: current.alert, alertLeads: current.alertLeads };
  const finishChange = beginCalendarChange();
  savingAlerts.add(item.key);
  update(item.key, patch);
  try {
    await save();
  } catch (e) {
    if (!isCurrentSession(version) || isStaleSession(e)) return;
    update(item.key, before);
    if (!handleAuthError(e)) toastOnce(errorText(e, '저장하지 못했어요.'), 'error');
  } finally {
    if (isCurrentSession(version)) savingAlerts.delete(item.key);
    finishChange();
  }
}

export async function setItemAlert(item: CalendarItem, on: boolean) {
  await saveItemAlert(item, { alert: on }, () => api.setAlert(item.key, on));
}

export async function setItemAlertLeads(item: CalendarItem, leads: number[] | null) {
  await saveItemAlert(item, { alertLeads: leads }, () => api.setAlertLeads(item.key, leads));
}


const KEY = 'downloads';
const load = () => readUserData<DownloadRecord[]>(KEY, []);

export const downloads = $state({ list: load(), busy: '' });

function remember(record: DownloadRecord) {
  downloads.list = [record, ...downloads.list.filter((d) => d.id !== record.id || d.path !== record.path)].slice(0, 30);
  writeUserData(KEY, downloads.list);
}

onSessionChange(() => {
  downloads.list = load();
  downloads.busy = '';
  saving.clear();
  savingAlerts.clear();
  cancelDoneConfirmation();
});

export function fileId(src: FileSource): string {
  if (src.kind === 'assign') return `${src.cmid}:${src.index}`;
  if (src.kind === 'module') return `m:${src.cmid}:${src.index}`;
  return `b:${src.cmid}:${src.bwid}:${src.index}`;
}

export function savedFile(src: FileSource): DownloadRecord | undefined {
  const id = fileId(src);
  return downloads.list.find((d) => d.id === id && (d.path || !isApp));
}

async function browserDownload(src: FileSource, name: string) {
  const blob = await inSession(async () => {
    let res: Response;
    try {
      res = await fetch(api.fileUrl(src), { credentials: 'same-origin' });
    } catch {
      reportServer(false);
      throw connectionError(false, { method: 'GET', path: api.fileUrl(src) });
    }
    if (!res.ok) {
      const body = await res.json().catch(() => null);
      throw responseError(res.status, body, { method: 'GET', path: api.fileUrl(src), format: responseFormat(res.headers.get('content-type')) });
    }
    return res.blob();
  });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = name.split('/').pop() || name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

const fileMessages = new Set([
  '자동 로그인 정보를 읽지 못했어요.', '자동 로그인 정보를 저장하지 못했어요.', '자동 로그인 정보를 삭제하지 못했어요.',
  '다운로드 폴더를 찾지 못했어요.', '다운로드 폴더를 열지 못했어요.', '폴더를 만들지 못했어요.',
  '파일을 저장하지 못했어요.', '파일이 없어요.', '열 수 없는 경로예요.',
  '파일이나 링크를 열지 못했어요.', '링크를 열 브라우저가 없어요.', '이 파일 형식을 열 수 있는 앱이 없어요.',
  '파일 앱을 열지 못했어요.', '폴더를 열지 못했어요.', '파일을 다운받을 수 없어요.', '다운로드에 실패했어요.',
  '활동을 찾지 못했어요.', '글을 찾지 못했어요.', '로그인이 필요해요.', '다시 로그인해 주세요.',
  '학교 서버가 응답하지 않아요.', '인터넷에 연결되어 있지 않아요.', '홍시 서버에 연결할 수 없어요.',
  '동기화 서버에 연결할 수 없어요.', '서버에 연결하지 못했어요.', '응답이 늦어지고 있어요.',
  '서버 응답을 확인하지 못했어요.', '잠시 후 다시 시도해 주세요.', '이 요청은 허용되지 않았어요.',
  '파일이나 요청의 용량이 너무 커요.',
]);

export function fileErrorText(error: unknown, fallback: string): string {
  const text = errorText(error, fallback).trim();
  const message = text.endsWith('.') ? text : `${text}.`;
  return fileMessages.has(message) ? message : fallback;
}

export async function downloadFile(src: FileSource, name: string, course: string): Promise<DownloadRecord | null> {
  const version = sessionVersion();
  const id = fileId(src);
  downloads.busy = id;
  try {
    const path = isApp ? await native.download(src, name) : (await browserDownload(src, name), null);
    const record: DownloadRecord = {
      id,
      name: name.split('/').pop() || name,
      course,
      source: src,
      at: Date.now(),
      path,
    };
    remember(record);
    toast(isApp ? '다운로드 폴더 안의 ‘홍시’ 폴더에 저장했어요.' : '다운로드했어요.', 'success');
    return record;
  } catch (e) {
    if (!handleAuthError(e)) toastOnce(fileErrorText(e, '다운로드에 실패했어요.'), 'error');
    return null;
  } finally {
    if (isCurrentSession(version)) downloads.busy = '';
  }
}

export async function openDownload(d: DownloadRecord) {
  try {
    if (isApp && d.path) await native.openFile(d.path);
    else window.open(api.fileUrl(d.source, true), '_blank', 'noopener');
  } catch (e) {
    toast(fileErrorText(e, '파일이나 링크를 열지 못했어요.'), 'error');
  }
}

export async function revealDownload(d: DownloadRecord) {
  try {
    if (d.path) await native.revealFile(d.path);
  } catch (e) {
    toast(fileErrorText(e, '폴더를 열지 못했어요.'), 'error');
  }
}
