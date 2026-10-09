import { api, ApiError } from '../../shared/api/api';
import { beginCalendarChange } from '../calendar/calendar-sync.svelte';
import { errorText } from '../../shared/api/net.svelte';
import { isCurrentSession, onSessionChange, readUserData, sessionVersion, writeUserData } from '../../shared/state/session';
import { calendar } from '../calendar/calendar-resources.svelte';
import { handleAuthError } from '../auth/auth-state.svelte';
import { todos } from '../calendar/todo-resource.svelte';
import type { SubmissionJob } from '../../shared/types';

type Pending = { id: string; created: number };
export type SubmissionOperation = {
  cmid: number;
  job: SubmissionJob | null;
  delayed: boolean;
  checking: boolean;
  error: string;
  missing: boolean;
};
const operations = new Map<number, SubmissionOperation>();
const monitors = new Map<number, Promise<void>>();
const releases = new Map<number, () => void>();

function pending(cmid: number, value: Pending | null) {
  const all = readUserData<Record<string, Pending>>('submissions', {});
  for (const [key, entry] of Object.entries(all)) {
    if (!entry || Date.now() - entry.created > 30 * 60_000) delete all[key];
  }
  if (value) all[cmid] = value;
  else delete all[cmid];
  writeUserData('submissions', all);
}

function initial(id: string): SubmissionJob {
  return { id, revision: 0, status: 'running', error: null, result: null,
    progress: { stage: 'prepare', fileName: null, fileCount: 0, uploadedFiles: 0, sentBytes: 0, totalBytes: 0 } };
}

export function submissionOperation(cmid: number): SubmissionOperation {
  let operation = operations.get(cmid);
  if (!operation) {
    const saved = readUserData<Record<string, Pending>>('submissions', {})[cmid];
    const id = saved && /^[a-f0-9]{32}$/i.test(saved.id) && Date.now() - saved.created < 30 * 60_000 ? saved.id : null;
    const created = $state<SubmissionOperation>({ cmid, job: id ? initial(id) : null, delayed: !!id, checking: false, error: '', missing: false });
    operation = created;
    operations.set(cmid, operation);
  }
  return operation;
}

function release(operation: SubmissionOperation) {
  releases.get(operation.cmid)?.();
  releases.delete(operation.cmid);
}

function lock(operation: SubmissionOperation) {
  if (releases.has(operation.cmid)) return;
  const mutation = calendar.beginMutation();
  const change = beginCalendarChange();
  releases.set(operation.cmid, () => { mutation(); change(); });
}

function receive(operation: SubmissionOperation, snapshot: SubmissionJob) {
  if (operation.job?.id !== snapshot.id || snapshot.revision < operation.job.revision) return;
  if (operation.job.status !== 'running' && snapshot.status === 'running') return;
  const completed = operation.job.status === 'complete';
  operation.job = snapshot;
  operation.error = snapshot.error?.message ?? '';
  operation.delayed = snapshot.status === 'uncertain';
  if (snapshot.status === 'complete' && snapshot.result?.info.status === 'submitted') {
    const key = `assign:${operation.cmid}`;
    if (calendar.data) calendar.set({ ...calendar.data,
      items: calendar.data.items.map((item) => item.key === key ? { ...item, status: 'submitted', done: true, doneOverride: true } : item),
    });
    pending(operation.cmid, null);
    if (!completed) void todos.refresh();
  } else if (snapshot.status === 'failed') {
    pending(operation.cmid, null);
    if (snapshot.error) handleAuthError(new ApiError(snapshot.error.status, snapshot.error.code, snapshot.error.message));
  }
  if (snapshot.status !== 'running') release(operation);
}

function disconnected(operation: SubmissionOperation, error: unknown) {
  operation.delayed = true;
  operation.error = errorText(error, '제출 결과를 확인하지 못했어요.');
  operation.missing = error instanceof ApiError && error.status === 404;
  release(operation);
  handleAuthError(error);
}

async function monitor(operation: SubmissionOperation, verify = false) {
  if (monitors.has(operation.cmid)) return monitors.get(operation.cmid);
  const version = sessionVersion(), id = operation.job?.id;
  if (!id) return;
  const current = () => isCurrentSession(version) && operation.job?.id === id;
  const task = (async () => {
    const started = Date.now();
    operation.checking = true;
    try {
      do {
        const snapshot = await api.submissionJob(operation.cmid, id, verify);
        if (!current()) return;
        receive(operation, snapshot);
        operation.checking = false;
        verify = false;
        if (snapshot.status !== 'running') return;
        lock(operation);
        if (Date.now() - started > 240_000) throw new ApiError(0, 'timeout', '학교의 응답이 늦어지고 있어요.');
        await new Promise((resolve) => setTimeout(resolve, 800));
      } while (current() && operation.job?.status === 'running' && !operation.delayed);
    } catch (error) {
      if (current()) disconnected(operation, error);
    } finally {
      if (current()) operation.checking = false;
    }
  })();
  monitors.set(operation.cmid, task);
  await task;
  if (monitors.get(operation.cmid) === task) monitors.delete(operation.cmid);
}

export async function resumeSubmission(operation: SubmissionOperation) {
  if (!operation.job || operation.job.status === 'complete' || operation.job.status === 'failed') return;
  operation.error = '';
  operation.missing = false;
  await monitor(operation, true);
}

export async function sendSubmission(operation: SubmissionOperation, keep: string[], files: File[], late: boolean, statement: boolean) {
  if (operation.job && operation.job.status !== 'failed' && operation.job.status !== 'complete') return;
  const version = sessionVersion();
  const id = crypto.randomUUID().replaceAll('-', '');
  const current = () => isCurrentSession(version) && operation.job?.id === id;
  operation.job = initial(id);
  operation.delayed = false;
  operation.error = '';
  operation.missing = false;
  pending(operation.cmid, { id, created: Date.now() });
  lock(operation);
  try {
    const snapshot = await api.submit(operation.cmid, keep, files, late, statement, id, (snapshot) => {
      if (current()) receive(operation, snapshot);
    });
    if (!current()) return;
    receive(operation, snapshot);
    if (operation.job?.status === 'running') await monitor(operation);
  } catch (error) {
    if (!current()) return;
    if (error instanceof ApiError && error.status >= 400 && error.status < 500 && error.code !== 'school_retry_required') {
      receive(operation, { ...operation.job!, revision: operation.job!.revision + 1, status: 'failed', error: { status: error.status, code: error.code, message: error.message } });
      handleAuthError(error);
    } else disconnected(operation, error);
  }
}

export function clearSubmission(operation: SubmissionOperation) {
  release(operation);
  pending(operation.cmid, null);
  operation.job = null;
  operation.error = '';
  operation.delayed = false;
  operation.missing = false;
}

onSessionChange(() => {
  releases.forEach((release) => release());
  releases.clear();
  operations.clear();
  monitors.clear();
});
