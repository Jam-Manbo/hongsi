<script lang="ts">
  import { appUpdater as update, canUpdateApp } from '../lib/app-update.svelte';
  import { sentenceLines } from '../lib/format';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
</script>

{#if canUpdateApp}
  <Sheet bind:open={update.open} title="앱 업데이트" onclose={() => update.dismiss()}>
    <div class="update">
      {#if update.currentVersion}<p class="muted">현재 버전 {update.currentVersion}</p>{/if}
      {#if update.checking}
        <p role="status">새 버전을 확인하고 있어요…</p>
      {:else if update.release}
        <h3>홍시 {update.release.version}</h3>
        <p class="muted">{(update.release.size / 1024 / 1024).toFixed(1)} MB</p>
        {#if update.release.notes}<p class="notes">{update.release.notes}</p>{/if}
      {:else if update.checked}
        <p>{update.configured ? '최신 버전을 사용하고 있어요.' : '아직 공개된 업데이트가 없어요.'}</p>
      {/if}
      {#if update.error}<p class="error" role="alert">{sentenceLines(update.error)}</p>{/if}
    </div>
    {#snippet footer()}
      <button class="btn btn-ghost" onclick={() => update.dismiss()}>{update.release ? '나중에' : '닫기'}</button>
      {#if update.release}
        {#if update.permissionsNeeded}
          <button class="btn btn-primary" onclick={() => update.permissions()}>설치 허용 설정</button>
        {:else}
          <button class="btn btn-primary" disabled={update.installing || update.checking} onclick={() => update.install()}><Icon name="download" size={18} />{update.installing ? '다운로드 중…' : '업데이트'}</button>
        {/if}
      {:else if update.error}
        <button class="btn btn-primary" disabled={update.checking} onclick={() => update.check(true)}>다시 확인</button>
      {/if}
    {/snippet}
  </Sheet>
{/if}

<style>
  .update { display: grid; gap: 12px; line-height: 1.6; }
  h3 { font-size: 24px; }
  .notes { white-space: pre-wrap; overflow-wrap: anywhere; }
  .error { color: var(--danger); white-space: pre-line; }
  .btn { flex: 1 1 0; min-width: 0; }
  .btn-primary { border: 1px solid transparent; }
</style>
