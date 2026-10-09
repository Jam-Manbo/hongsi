<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { untrack } from 'svelte';
  import { app } from '../auth/auth-state.svelte';
  import { sentenceLines } from '../../shared/utils/format';
  import { studentCardRemaining } from '../../platform/student-card-time';
  import Icon from '../../shared/ui/Icon.svelte';
  import Sheet from '../../shared/ui/Sheet.svelte';

  type CardResult =
    | { state: 'ready'; imageDataUrl: string; validForMs: number; processingMs: number; timingId: number }
    | { state: 'authentication_required' | 'credentials_required' | 'unavailable'; message: string; retryable?: boolean };

  let { open = $bindable(false) }: { open: boolean } = $props();
  let image = $state('');
  let message = $state('');
  let loading = $state(false);
  let retryable = $state(false);
  let remaining = $state(0);
  let refresh = $state<() => void>(() => {});
  const ready = $derived(!loading && !!image && remaining > 0);

  $effect(() => {
    const owner = app.account;
    if (!open || !owner || app.loggingOut) return;
    return untrack(() => {
      let disposed = false;
      let viewId = '';
      let request = 0;
      let deadline = 0;
      let expires: ReturnType<typeof setTimeout> | undefined;
      let ticks: ReturnType<typeof setInterval> | undefined;
      let busy = false;
      let foreground = !document.hidden;

      function clearDisplay() {
        clearTimeout(expires);
        clearInterval(ticks);
        image = '';
        remaining = 0;
        deadline = 0;
      }
      function suspend() {
        request++;
        busy = false;
        loading = false;
        clearDisplay();
        const previous = viewId;
        viewId = '';
        if (previous) void invoke('student_card_close', { viewId: previous }).catch(() => {});
      }
      async function load(fresh: boolean) {
        if (disposed || !foreground || document.hidden || busy || app.account !== owner || app.loggingOut) return;
        if (fresh) { suspend(); viewId = crypto.randomUUID(); }
        if (!viewId) return;
        busy = true;
        loading = true;
        message = '';
        retryable = false;
        clearDisplay();
        const current = ++request;
        const started = performance.now();
        try {
          const result = await invoke<CardResult>(fresh ? 'student_card_open' : 'student_card_refresh', { viewId });
          if (disposed || current !== request || !foreground || document.hidden || app.account !== owner || app.loggingOut) return;
          if (result.state === 'ready') {
            const elapsed = performance.now() - started;
            const validFor = studentCardRemaining(result.validForMs, result.processingMs, elapsed);
            if (!Number.isFinite(validFor) || validFor <= 0 || !result.imageDataUrl.startsWith('data:image/png;base64,')) {
              message = 'QR 유효시간이 지났어요. 다시 발급해 주세요.';
              retryable = true;
              return;
            }
            image = result.imageDataUrl;
            deadline = performance.now() + validFor;
            remaining = Math.ceil(validFor / 1000);
            void invoke('student_card_displayed', { viewId, timingId: result.timingId, elapsedMs: Math.round(elapsed), remainingMs: Math.round(validFor) }).catch(() => {});
            ticks = setInterval(() => {
              remaining = Math.max(0, Math.ceil((deadline - performance.now()) / 1000));
              if (!remaining) { clearDisplay(); void load(false); }
            }, 200);
            expires = setTimeout(() => { clearDisplay(); void load(false); }, validFor);
          } else {
            message = result.message;
            retryable = result.state === 'authentication_required' || result.retryable === true;
          }
        } catch {
          if (disposed || current !== request) return;
          message = '학생증 QR을 불러오지 못했어요. 잠시 후 다시 시도해 주세요.';
          retryable = true;
        } finally {
          if (current === request) { busy = false; loading = false; }
        }
      }
      const visibility = () => { if (document.hidden) suspend(); else if (foreground && !viewId) void load(true); };
      const pause = () => { foreground = false; suspend(); };
      const resume = () => { foreground = true; if (!viewId) void load(true); };
      refresh = () => { void load(!image); };
      document.addEventListener('visibilitychange', visibility);
      window.addEventListener('hongsi-pause', pause);
      window.addEventListener('hongsi-resume', resume);
      void load(true);
      return () => {
        disposed = true;
        suspend();
        refresh = () => {};
        message = '';
        document.removeEventListener('visibilitychange', visibility);
        window.removeEventListener('hongsi-pause', pause);
        window.removeEventListener('hongsi-resume', resume);
      };
    });
  });
</script>

<Sheet bind:open title="학생증 QR" titleIcon="qr">
  <div class="student-card" aria-busy={loading}>
    <div class="identity"><strong>{app.profile?.name || '모바일 학생증'}</strong><span class="muted">{app.profile?.studentId || app.account}</span></div>
    <div class="qr-frame" class:pending={!ready} aria-hidden={!ready ? 'true' : undefined}>
      {#if ready}<img src={image} alt="학생증 QR 코드" draggable="false" />{/if}
    </div>
    <div class="status-copy" role="status">
      {#if loading || ready}
        <p class="countdown">
          {#if loading}<span class="spinner" aria-hidden="true"></span>학생증 QR을 불러오는 중…
          {:else}남은 시간 <strong>{remaining}초</strong>{/if}
        </p>
        <p class="hint muted">시간이 지나면 자동으로 새 QR을 받아요.</p>
      {:else}
        <p class="message">{sentenceLines(message || '학생증 QR을 준비하고 있어요.')}</p>
      {/if}
    </div>
    <div class="action">
      {#if loading || ready}
        <button class="btn btn-ghost" disabled={loading} onclick={refresh}><Icon name="refresh" size={18} />새로고침</button>
      {:else if retryable}
        <button class="btn btn-primary" onclick={refresh}><Icon name="refresh" size={18} />다시 시도</button>
      {/if}
    </div>
  </div>
</Sheet>

<style>
  .student-card { display: grid; justify-items: center; gap: 16px; padding: 12px 0 8px; text-align: center; }
  .identity { display: grid; gap: 4px; overflow-wrap: anywhere; max-width: 100%; }
  .identity strong { font-size: 20px; }
  .identity span { font-size: 14px; font-variant-numeric: tabular-nums; }
  .qr-frame { width: min(100%, 288px); aspect-ratio: 1; background: #fff; border-radius: 16px; padding: 8px; }
  .qr-frame.pending { background: transparent; border: 2px dashed var(--border-strong); padding: 6px; }
  .qr-frame img { display: block; width: 100%; height: 100%; image-rendering: pixelated; }
  .countdown { display: flex; align-items: center; justify-content: center; gap: 8px; min-height: 24px; font-size: 15px; line-height: 24px; font-variant-numeric: tabular-nums; }
  .countdown strong { color: var(--primary); }
  .status-copy { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; min-height: 52px; width: 100%; min-width: 0; }
  .hint { font-size: 13px; line-height: 20px; }
  .message { width: 100%; font-size: 14px; line-height: 22px; color: var(--text-2); white-space: pre-line; word-break: keep-all; overflow-wrap: anywhere; }
  .action { min-height: 44px; }
  .spinner { flex: none; width: 16px; height: 16px; border: 2px solid var(--border); border-top-color: var(--primary); border-radius: 50%; animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .spinner { animation: none; } }
</style>
