import { clearReminders, scheduleReminders } from './notify';
import { pref, setPref } from './store.svelte';
import type { SeatPeriod, SeatSession } from './types';

export const PERIODS: { id: SeatPeriod; label: string; hours: number }[] = [
  { id: 'semester', label: '학기 중', hours: 6 },
  { id: 'exam', label: '시험 기간', hours: 4 },
  { id: 'vacation', label: '방학', hours: 8 },
];

export const ALERT_CHOICES = [60, 30, 10, 0];

export const seatPrefs = $state({
  alerts: pref<number[]>('seat-alerts', [30, 10]),
  period: pref<SeatPeriod>('seat-period', 'semester'),
});

export function toggleAlert(min: number) {
  seatPrefs.alerts = seatPrefs.alerts.includes(min)
    ? seatPrefs.alerts.filter((m) => m !== min)
    : [...seatPrefs.alerts, min].sort((a, b) => b - a);
  setPref('seat-alerts', seatPrefs.alerts);
}

export function setPeriod(p: SeatPeriod) {
  seatPrefs.period = p;
  setPref('seat-period', p);
}

export function seatLabel(s: SeatSession) {
  return `${s.buildingName} ${s.roomName} ${s.seatNo}번`;
}

export function syncSeatReminders(session: SeatSession | null) {
  if (!session) {
    void clearReminders('seat');
    return;
  }
  const end = session.expiresAt * 1000;
  void scheduleReminders(
    'seat',
    seatPrefs.alerts.map((min) => ({
      key: `seat:${session.id}:${end}:${min}`,
      target: { kind: 'seat' as const, id: session.id },
      at: end - min * 60_000,
      title: min === 0 ? '좌석 이용 시간이 끝났어요' : `좌석 이용 종료까지 ${min}분 남았어요`,
      body: '',
    })),
  );
}
