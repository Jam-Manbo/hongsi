import { emptyStatus, ID_BASE, reminderPlan, type Channel, type ChannelStatus, type NotificationIntent, type Permission, type Reminder } from './notification-model';
export interface NotificationDriver {
  native: boolean;
  ios: boolean;
  permission(ask: boolean): Promise<Permission>;
  pending(): Promise<number[]>;
  cancel(ids: number[]): Promise<void>;
  send(id: number, reminder: Reminder, intent: NotificationIntent, scheduled: boolean): Promise<void>;
}
type Status = (channel: Channel, value: ChannelStatus) => void;
const CHANNELS: Channel[] = ['seat', 'due'];
const MAX_DELAY = 2_000_000_000;

export class ReminderScheduler {
  private plans: Record<Channel, Reminder[]> = { seat: [], due: [] };
  private timers: Record<Channel, ReturnType<typeof setTimeout>[]> = { seat: [], due: [] };
  private signatures: Partial<Record<Channel, string>> = {};
  private tail: Promise<void> = Promise.resolve();
  private revision = 0;
  private account: string | null = null;
  enabled = true;
  remote = false;
  permission: Permission = 'unknown';
  constructor(private driver: NotificationDriver, private status: Status, private onPermission: (p: Permission) => void) {}

  reset(account: string | null) {
    this.revision++;
    this.account = account;
    this.plans = { seat: [], due: [] };
    this.signatures = {};
    for (const c of CHANNELS) { this.clearTimers(c); this.status(c, emptyStatus()); }
    return this.refresh(true);
  }
  set(channel: Channel, reminders: Reminder[]) {
    this.plans[channel] = reminders;
    return this.refresh();
  }
  async allow(ask = false) {
    this.permission = await this.driver.permission(ask);
    this.onPermission(this.permission);
    await this.refresh(true);
    return this.permission === 'granted';
  }
  refresh(force = false): Promise<void> {
    const rev = this.revision;
    const run = this.tail.then(async () => {
      if (rev !== this.revision) return;
      try {
        this.permission = await this.driver.permission(false);
        if (rev !== this.revision) return;
        this.onPermission(this.permission);
        for (const c of CHANNELS) {
          if (rev !== this.revision) return;
          await this.reconcile(c, rev, force);
        }
      } catch {
        if (rev === this.revision) for (const c of CHANNELS) this.status(c, { ...emptyStatus(), error: '알림 설정을 확인하지 못했어요. 잠시 후 다시 시도해 주세요.' });
      }
    });
    this.tail = run.catch(() => {});
    return run;
  }
  private clearTimers(c: Channel) { this.timers[c].forEach(clearTimeout); this.timers[c] = []; }
  private async reconcile(c: Channel, rev: number, force: boolean) {
    const active = this.account && this.enabled && !this.remote;
    const limit = c === 'seat' ? 6 : this.driver.ios ? 56 : 400;
    const { all, selected, deferred } = reminderPlan(active ? this.plans[c] : [], limit, Date.now(), this.driver.native ? Infinity : MAX_DELAY);
    const allowed = this.permission === 'granted';
    const plan = allowed ? selected : [];
    const signature = JSON.stringify([this.account, this.enabled, this.remote, this.permission, all.length, deferred, plan]);
    if (!force && this.signatures[c] === signature) return;
    const result: ChannelStatus = { planned: all.length, scheduled: 0, deferred, next: null, checkedAt: Date.now(), error: '' };
    this.clearTimers(c);
    const base = ID_BASE[c];
    try {
      if (this.driver.native) {
        const pending = await this.driver.pending();
        if (rev !== this.revision) return;
        const mine = pending.filter((id) => id >= base && id < base + 500);
        if (mine.length) await this.driver.cancel(mine);
        if (rev !== this.revision) return;
        for (const [i, r] of plan.entries()) {
          await this.driver.send(base + i, r, this.intent(r), true);
          if (rev !== this.revision) return;
        }
        const verified = new Set(await this.driver.pending());
        if (rev !== this.revision) return;
        const accepted = plan.filter((_, i) => verified.has(base + i));
        result.scheduled = accepted.length;
        result.next = accepted[0]?.at ?? null;
        if (accepted.length !== plan.length) throw new Error('missing');
      } else {
        for (const [i, r] of plan.entries()) {
          const intent = this.intent(r);
          this.timers[c].push(setTimeout(() => {
            if (rev !== this.revision || !this.enabled || this.remote) return;
            if (Date.now() - r.at <= 10 * 60_000) {
              void this.driver.send(base + i, r, intent, false).catch(() => this.status(c, { ...result, error: '알림을 표시하지 못했어요. 기기 알림 설정을 확인해 주세요.' }));
            }
            void this.refresh();
          }, Math.max(0, r.at - Date.now())));
        }
        result.scheduled = plan.length;
        result.next = plan[0]?.at ?? null;
      }
      this.signatures[c] = signature;
    } catch {
      delete this.signatures[c];
      result.error = '예약된 알림을 갱신하지 못했어요. 잠시 후 다시 시도해 주세요.';
    }
    if (rev === this.revision) this.status(c, result);
  }
  private intent(r: Reminder): NotificationIntent { return { version: 1, account: this.account!, target: r.target, at: r.at }; }
}
