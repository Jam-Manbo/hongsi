import { ApiError } from '../api/api';
import { app, handleAuthError } from '../../features/auth/auth-state.svelte';
import { errorText, troubleOf, type Trouble } from '../api/net.svelte';
import { readUserData, writeUserData } from './session';

type Cached<T> = { data: T; at: number };

function readCache<T>(key: string): Cached<T> | null {
  return readUserData<Cached<T> | null>(key, null);
}

function writeCache<T>(key: string, data: T, at: number) {
  writeUserData(key, { data, at });
}

export class Resource<T> {
  data = $state<T | null>(null);
  at = $state(0);
  loading = $state(false);
  error = $state<string | null>(null);
  trouble = $state<Trouble | 'other' | null>(null);
  private lastStart = 0;
  private revision = 0;
  private generation = 0;
  private mutations = 0;
  private pending: Promise<void> | null = null;
  private refreshAfterMutation = false;

  constructor(
    private key: string,
    private fetcher: (force: boolean) => Promise<T>,
    private maxAgeMs: number,
    private normalize: (data: T) => T = (data) => data,
  ) {}

  restore() {
    this.reset();
    const cached = readCache<T>(this.key);
    if (cached) {
      this.data = this.normalize(cached.data);
      this.at = cached.at;
    }
  }

  setKey(key: string) {
    if (this.key === key) return;
    this.key = key;
    this.restore();
  }

  get stale() {
    return Date.now() - this.at > this.maxAgeMs;
  }

  load(force = false): Promise<void> {
    if (app.loggingOut || this.mutations) return Promise.resolve();
    if (this.pending) return this.pending;
    if (!force && this.data !== null && !this.stale) return Promise.resolve();
    if (Date.now() - this.lastStart < 3000) return Promise.resolve();
    this.lastStart = Date.now();
    const revision = this.revision;
    this.loading = true;
    const pending = this.fetch(force, revision);
    this.pending = pending;
    void pending.finally(() => { if (this.pending === pending) this.pending = null; });
    return pending;
  }

  refresh(): Promise<void> {
    if (this.mutations) {
      this.refreshAfterMutation = true;
      return Promise.resolve();
    }
    this.revision += 1;
    this.pending = null;
    this.lastStart = 0;
    return this.load(true);
  }

  private async fetch(force: boolean, revision: number) {
    try {
      const data = await this.fetcher(force);
      if (revision !== this.revision) return;
      this.set(data);
      this.error = null;
      this.trouble = null;
    } catch (e) {
      if (revision !== this.revision) return;
      if (!handleAuthError(e)) {
        this.trouble = (e instanceof ApiError ? troubleOf(e.status, e.code) : null) ?? 'other';
        this.error = errorText(e, '불러오지 못했어요.');
      }
    } finally {
      if (revision === this.revision) this.loading = false;
    }
  }

  set(data: T, at = Date.now()) {
    data = this.normalize(data);
    this.data = data;
    this.at = at;
    writeCache(this.key, data, this.at);
  }

  beginMutation() {
    const generation = this.generation;
    this.mutations += 1;
    this.revision += 1;
    this.pending = null;
    this.loading = false;
    let finished = false;
    return () => {
      if (finished || generation !== this.generation) return;
      finished = true;
      this.mutations -= 1;
      this.revision += 1;
      this.lastStart = 0;
      if (!this.mutations && this.refreshAfterMutation) {
        this.refreshAfterMutation = false;
        void this.refresh();
      }
    };
  }

  reset() {
    this.generation += 1;
    this.mutations = 0;
    this.refreshAfterMutation = false;
    this.revision += 1;
    this.pending = null;
    this.lastStart = 0;
    this.loading = false;
    this.data = null;
    this.at = 0;
    this.error = null;
    this.trouble = null;
  }
}
