<script lang="ts">
  import { request } from '../lib/api';
  import { errorText } from '../lib/net.svelte';
  import { app, allResources, startSession } from '../lib/store.svelte';
  import { inSession } from '../lib/session';
  import { refreshBackground } from '../lib/background.svelte';
  import { toast } from '../lib/ui.svelte';
  import Sheet from './Sheet.svelte';
  import Icon from './Icon.svelte';

  let password = $state('');
  let visible = $state(false);
  let busy = $state(false);
  let error = $state('');
  $effect(() => { if (!app.schoolLoginOpen) { password = ''; error = ''; visible = false; } });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy || !password) return;
    busy = true;
    error = '';
    try {
      await request('POST', '/api/auth/reconnect', { password });
      if (!app.profile) return;
      startSession(app.profile, app.remembered, app.account ?? '');
      password = '';
      toast('학교에 다시 로그인했어요', 'success');
      await inSession(async (check) => {
        await Promise.all(allResources.map((resource) => resource.load(true)));
        check();
        await refreshBackground(true);
      });
    } catch (e) {
      error = errorText(e, '학교에 로그인하지 못했어요');
    } finally {
      busy = false;
    }
  }
</script>

<Sheet bind:open={app.schoolLoginOpen} title="학교에 다시 로그인">
  <form id="school-reconnect" onsubmit={submit}>
    <p>저장된 학교 인증이 만료됐어요. 비밀번호를 입력하면 다시 연결합니다.</p>
    <label>학번<input value={app.account ?? ''} autocomplete="username" readonly /></label>
    <label>비밀번호
      <div class="password">
        <input bind:value={password} type={visible ? 'text' : 'password'} autocomplete="current-password" placeholder="통합 로그인 비밀번호" required />
        <button type="button" onclick={() => visible = !visible} aria-label={visible ? '비밀번호 숨기기' : '비밀번호 보기'}><Icon name={visible ? 'eye-off' : 'eye'} size={20} /></button>
      </div>
    </label>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </form>
  {#snippet footer()}<button form="school-reconnect" class="btn btn-primary btn-block" disabled={busy || !password}>{busy ? '로그인하는 중…' : '다시 로그인'}</button>{/snippet}
</Sheet>

<style>
  form, label { display: grid; gap: 12px; }
  form { gap: 20px; }
  p { color: var(--text-2); line-height: 1.65; font-size: 14px; }
  label { font-size: 14px; font-weight: 650; }
  input { width: 100%; min-width: 0; padding: 13px 14px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface-2); color: var(--text); font: inherit; }
  input:focus { outline: 2px solid var(--accent); outline-offset: 2px; }
  input[readonly] { color: var(--text-2); }
  .password { display: flex; align-items: center; position: relative; }
  .password input { padding-right: 48px; }
  .password button { position: absolute; right: 4px; display: grid; place-items: center; width: 40px; height: 40px; color: var(--text-2); }
  .error { color: var(--danger); }
</style>
