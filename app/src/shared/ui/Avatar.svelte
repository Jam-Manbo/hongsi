<script lang="ts">
  import { onMount } from 'svelte';
  import { isApp, native } from '../api/api';
  import { app } from '../../features/auth/auth-state.svelte';

  let { size = 36 }: { size?: number } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);

  onMount(async () => {
    if (!app.profile?.hasPicture) return;
    src = isApp ? await native.avatar().catch(() => null) : '/api/me/avatar';
  });
</script>

<span class="avatar" aria-hidden="true" style:width="{size}px" style:height="{size}px">
  {#if src && !failed}
    <img {src} alt="" onerror={() => (failed = true)} />
  {:else}
    <img src="/favicon.svg" alt="" />
  {/if}
</span>

<style>
  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    overflow: hidden;
    border-radius: 999px;
    background: var(--surface);
    box-shadow: 0 0 0 2px var(--surface), 0 0 0 3px var(--border);
  }

  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
