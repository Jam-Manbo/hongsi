import { isApp } from './env';

export const canUpdateApp = isApp && /Android/i.test(navigator.userAgent);
export type AppRelease = { version: string | null; versionCode: number; size: number | null; notes: string };
type Info = { source: 'direct' | 'play'; currentVersion: string; currentCode: number; canInstall: boolean; installError: string | null };
type Check = Info & { configured: boolean; release: AppRelease | null };
type Install = { state: 'permission-required' | 'installer-opened' | 'play-completed' | 'cancelled' };
type Driver = <T>(action: string, versionCode?: number) => Promise<T>;
const problem = (error: unknown) => error instanceof Error ? error.message : String(error);
const saved = (key: string) => { try { return localStorage.getItem(`hc:pref:update-${key}`); } catch { return null; } };
const save = (key: string, value: string) => { try { localStorage.setItem(`hc:pref:update-${key}`, value); } catch {   } };

export class AppUpdater {
  open = $state(false);
  checking = $state(false);
  installing = $state(false);
  permissionsNeeded = $state(false);
  installerOpened = $state(false);
  source = $state<'direct' | 'play'>('direct');
  currentVersion = $state('');
  currentCode = $state(0);
  release = $state<AppRelease | null>(null);
  configured = $state(true);
  checked = $state(false);
  error = $state('');
  private attemptedAt = 0;
  constructor(private driver: Driver) {}
  async status() {
    try {
      const info = await this.driver<Info>('status');
      this.source = info.source;
      this.currentVersion = info.currentVersion;
      this.currentCode = info.currentCode;
      this.permissionsNeeded = !info.canInstall;
      if (info.installError) { this.error = info.installError; this.installerOpened = false; }
    } catch {   }
  }
  async check(manual = false) {
    if (manual) this.open = true;
    if (this.checking || this.installing) return;
    await this.status();
    if (this.checking || this.installing) return;
    const now = Date.now();
    if (!manual && now - Math.max(this.attemptedAt, Number(saved(`checked-at-${this.source}`)) || 0) < 6 * 3600_000) return;
    this.attemptedAt = now;
    this.checking = true; this.error = ''; this.installerOpened = false;
    try {
      const result = await this.driver<Check>('check');
      this.source = result.source;
      this.currentVersion = result.currentVersion; this.currentCode = result.currentCode;
      this.release = result.release; this.configured = result.configured;
      this.permissionsNeeded = !result.canInstall;
      this.checked = true;
      save(`checked-at-${this.source}`, String(now));
      if (manual || (result.release && saved(`dismissed-${this.source}`) !== String(result.release.versionCode))) this.open = true;
    } catch (e) { this.release = null; this.checked = false; this.error = problem(e); }
    finally { this.checking = false; }
  }
  dismiss() {
    if (this.release) save(`dismissed-${this.source}`, String(this.release.versionCode));
    this.open = false;
  }
  async permissions() {
    try { await this.driver('permissions'); } catch (e) { this.error = problem(e); }
  }
  async install() {
    if (!this.release || this.installing || this.checking) return;
    this.installing = true; this.error = ''; this.installerOpened = false;
    try {
      const result = await this.driver<Install>('install', this.release.versionCode);
      this.permissionsNeeded = result.state === 'permission-required';
      this.installerOpened = result.state === 'installer-opened';
      if (result.state === 'play-completed') { this.release = null; this.open = false; }
    } catch (e) { this.error = problem(e); }
    finally { this.installing = false; }
  }
}

export const appUpdater = new AppUpdater(async <T>(action: string, versionCode?: number): Promise<T> => {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>('app_update', { action, versionCode });
});

export function watchAppUpdates() {
  if (!canUpdateApp) return () => {};
  void appUpdater.status();
  const timer = setTimeout(() => { void appUpdater.check(); }, 3000);
  const resume = () => { void appUpdater.status(); if (!appUpdater.open) void appUpdater.check(); };
  window.addEventListener('focus', resume);
  const visible = () => { if (document.visibilityState === 'visible') resume(); };
  document.addEventListener('visibilitychange', visible);
  return () => { clearTimeout(timer); window.removeEventListener('focus', resume); document.removeEventListener('visibilitychange', visible); };
}
