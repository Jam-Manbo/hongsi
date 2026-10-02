import { ApiError, api, isApp, native } from './api';
import { errorText, reportServer, writeBlocked } from './net.svelte';
import { schoolFinished } from './colors';
import { calendar, handleAuthError } from './store.svelte';
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
  saving.add(item.key);
  update(item.key, { done, doneOverride: override });
  try {
    await api.setDone(item.key, override);
  } catch (e) {
    if (isStaleSession(e)) return;
    update(item.key, before);
    if (!handleAuthError(e)) toastOnce(errorText(e, '저장하지 못했어요'), 'error');
  } finally {
    if (isCurrentSession(version)) saving.delete(item.key);
  }
}


async function saveItemAlert(item: CalendarItem, patch: Partial<CalendarItem>, save: () => Promise<unknown>) {
  if (writeBlocked() || savingAlerts.has(item.key)) return;
  const version = sessionVersion();
  const current = calendar.data?.items.find((i) => i.key === item.key) ?? item;
  const before = { alert: current.alert, alertLeads: current.alertLeads ?? null };
  savingAlerts.add(item.key);
  update(item.key, patch);
  try {
    await save();
  } catch (e) {
    if (!isCurrentSession(version) || isStaleSession(e)) return;
    update(item.key, before);
    if (!handleAuthError(e)) toastOnce(errorText(e, '저장하지 못했어요'), 'error');
  } finally {
    if (isCurrentSession(version)) savingAlerts.delete(item.key);
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

export function clearDownloads() {
  downloads.list = [];
  writeUserData(KEY, []);
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

const sourceOf = (d: DownloadRecord): FileSource => d.source ?? { kind: 'assign', cmid: d.cmid, index: d.index };

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
      throw new ApiError(0, 'offline', '서버에 연결하지 못했어요');
    }
    if (!res.ok) {
      const body = await res.json().catch(() => null);
      throw new ApiError(res.status, body?.error?.code ?? 'error', body?.error?.message ?? '파일을 받지 못했어요');
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
      cmid: src.cmid,
      index: src.index,
      at: Date.now(),
      path,
    };
    remember(record);
    toast(isApp ? '다운로드 폴더 안의 ‘홍시’ 폴더에 저장했어요' : '다운로드했어요', 'success');
    return record;
  } catch (e) {
    if (!handleAuthError(e)) toastOnce(errorText(e, '파일을 받지 못했어요'), 'error');
    return null;
  } finally {
    if (isCurrentSession(version)) downloads.busy = '';
  }
}

export function downloadAttachment(item: CalendarItem, index: number, course: string) {
  return downloadFile({ kind: 'assign', cmid: Number(item.key.split(':')[1]), index }, item.attachments[index].name, course);
}

export async function openDownload(d: DownloadRecord) {
  try {
    if (isApp && d.path) await native.openFile(d.path);
    else window.open(api.fileUrl(sourceOf(d), true), '_blank', 'noopener');
  } catch (e) {
    toast(e instanceof Error ? e.message : '열지 못했어요', 'error');
  }
}

export async function revealDownload(d: DownloadRecord) {
  try {
    if (d.path) await native.revealFile(d.path);
  } catch (e) {
    toast(e instanceof Error ? e.message : '폴더를 열지 못했어요', 'error');
  }
}
