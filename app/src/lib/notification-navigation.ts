import { tick } from 'svelte';
import { closeSheets } from '../components/Sheet.svelte';
import { app, calendar, notices, seatSession, todos } from './store.svelte';
import { focus, go, toast } from './ui.svelte';
import { sessionVersion, isCurrentSession } from './session';
import type { NotificationIntent } from './notification-model';
import { classroomDestination } from './classroom-notice';
import { displayedTodos } from './todos.svelte';

let navigation = 0;

export async function openNotification(intent: NotificationIntent) {
  const attempt = ++navigation;
  if (intent.account !== app.account) { toast('다른 계정의 알림이에요.', 'error'); return; }
  const version = sessionVersion(), target = intent.target;
  const current = () => attempt === navigation && isCurrentSession(version) && !app.loggingOut;
  const retry = () => { if (current()) void openNotification(intent); };
  if (!closeSheets()) {
    toast('진행 중인 작업이 끝나면 이 안내를 눌러 알림을 열어 주세요.', 'info', 8000, retry);
    return;
  }
  await tick();
  if (!current()) return;
  if (target.kind === 'notices') {
    const destination = target.url ? classroomDestination(target.url) : null;
    if (destination?.kind === 'item') {
      await openNotification({ ...intent, target: destination });
    } else if (target.url && destination) {
      // The article may have fallen out of the latest notification list.
      // Its school URL still identifies the authenticated detail request.
      focus.notice = notices.data?.find((n) => n.url === target.url) ?? {
        url: target.url, course: '클래스룸', section: '', when: '', message: '', kind: destination.kind,
      };
    } else {
      focus.notices = true;
    }
    return;
  }

  go(target.kind === 'seat' ? 'seats' : 'calendar');
  const resource = target.kind === 'item' ? calendar : target.kind === 'todo' ? todos : seatSession;
  const openDetail = () => {
    if (target.kind === 'item') {
      const item = calendar.data?.items.find((i) => i.key === target.key);
      if (!item) return false;
      if (item.done) toast('이미 완료한 일정이에요.', 'info');
      focus.item = target.key;
      return true;
    }
    if (target.kind === 'todo') {
      const todo = displayedTodos().find((t) => t.id === target.id);
      if (!todo) return false;
      if (todo.doneAt !== null) toast('이미 완료한 할 일이에요.', 'info');
      focus.todo = target.id;
      return true;
    }
    return false;
  };

  // Opening a known target must not depend on the connection recovering after resume.
  if (openDetail()) { void resource.load(true); return; }
  await resource.load(true);
  if (!current()) return;
  if (resource.error || !resource.data) {
    toast('알림 정보를 불러오지 못했어요. 연결 후 이 안내를 눌러 다시 시도해 주세요.', 'error', 8000, retry);
    return;
  }
  if (target.kind === 'seat') {
    const seat = seatSession.data?.session;
    if (!seat || seat.id !== target.id || seat.endedAt !== null) toast('현재 이용 중인 좌석이 아니에요.', 'info');
  } else if (!openDetail()) {
    toast(target.kind === 'item' ? '표시할 일정이 없어요.' : '표시할 할 일이 없어요.', 'info');
  }
}
