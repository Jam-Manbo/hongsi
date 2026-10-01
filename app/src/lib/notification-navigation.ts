import { app, calendar, seatSession, todos } from './store.svelte';
import { focus, go, toast } from './ui.svelte';
import { sessionVersion, isCurrentSession } from './session';
import type { NotificationIntent } from './notification-model';
export async function openNotification(intent: NotificationIntent) {
  if (intent.account !== app.account) { toast('다른 계정의 알림이에요. 해당 계정으로 로그인해 주세요.', 'error'); return; }
  const version = sessionVersion(), target = intent.target;
  if (target.kind === 'notices') { focus.notices = true; return; }
  const resource = target.kind === 'item' ? calendar : target.kind === 'todo' ? todos : seatSession;
  await resource.load(true);
  if (!isCurrentSession(version)) return;
  if (resource.error || !resource.data) { toast('알림의 최신 상태를 불러오지 못했어요.', 'error'); return; }
  if (target.kind === 'seat') {
    go('seats');
    const seat = seatSession.data?.session;
    if (!seat || seat.id !== target.id || seat.endedAt !== null) toast('만료된 좌석 이용입니다.', 'info');
  } else {
    if (target.kind === 'item') {
      const item = calendar.data?.items.find((i) => i.key === target.key);
      if (!item) { go('calendar'); toast('이 일정은 삭제됐거나 이번 학기 목록에 없어요.', 'info'); return; }
      if (item.done) toast('이미 완료한 일정이에요.', 'info');
      focus.item = target.key;
    } else {
      const todo = todos.data?.find((t) => t.id === target.id);
      if (!todo) { go('calendar'); toast('이 할 일은 삭제됐거나 보관 기간이 지났어요.', 'info'); return; }
      if (todo.doneAt !== null) toast('이미 완료한 할 일이에요.', 'info');
      focus.todo = target.id;
    }
    go('calendar');
  }
}
