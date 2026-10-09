import { handleAuthError } from '../auth/auth-state.svelte';
import { api, isApp, native } from '../../shared/api/api';
import { isIOS } from '../../platform/env';
import { connectionError, responseError, responseFormat } from '../../shared/api/api-error';
import { errorText, reportServer } from '../../shared/api/net.svelte';
import { inSession, isCurrentSession, sessionVersion, onSessionChange, readUserData, writeUserData } from '../../shared/state/session';
import { toast, toastOnce } from '../../shared/state/ui.svelte';
import type { DownloadRecord, FileSource } from '../../shared/types';

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
    toast(isIOS ? '파일 앱의 ‘홍시’에 저장했어요.' : isApp ? '다운로드 폴더 안의 ‘홍시’ 폴더에 저장했어요.' : '다운로드했어요.', 'success');
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
