<script lang="ts">
  import { ago } from '../lib/format';
  import { TROUBLE_TEXT, type Trouble } from '../lib/net.svelte';
  import Icon from './Icon.svelte';

  type Loadable = {
    data: unknown;
    at: number;
    error: string | null;
    trouble: Trouble | 'other' | null;
    loading: boolean;
    load: (force?: boolean) => Promise<void>;
  };

  let {
    resource,
    what,
    stale = true,
    hideParseError = false,
  }: {
    resource: Loadable;
    what: string;
    stale?: boolean;
    hideParseError?: boolean;
  } = $props();

  const parseError = $derived(resource.error?.includes('학교 페이지 형식을 읽지 못했어요') ?? false);
  const kind = $derived(resource.trouble);
  const conn = $derived(kind === 'offline' || kind === 'server' || kind === 'school' ? kind : null);
  const ICON: Record<Trouble, string> = { offline: 'wifi-off', server: 'cloud-off', school: 'alert' };
</script>

{#if resource.error}
  {#if resource.data !== null}
    {#if stale && resource.at}
      <p class="stale" class:failed={parseError}>
        <Icon name="clock" size={13} stroke={2} />
        {ago(resource.at / 1000)} 정보{conn ? '' : ` · ${parseError ? '불러오기를 실패했어요' : resource.error}`}
      </p>
    {/if}
  {:else}
    <div class="retry" role="alert">
      <span class="ico"><Icon name={conn ? ICON[conn] : 'alert'} size={20} /></span>
      <div class="txt">
        <strong>{what} 불러오지 못했어요</strong>
        {#if !(hideParseError && parseError)}<span>{conn ? TROUBLE_TEXT[conn].title : resource.error}</span>{/if}
      </div>
      <button class="btn btn-ghost again" onclick={() => resource.load(true)} disabled={resource.loading}>
        <span class:spin={resource.loading}><Icon name="refresh" size={16} stroke={2.2} /></span>다시 시도
      </button>
    </div>
  {/if}
{/if}

<style>
  .stale {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0 4px 10px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-3);
  }

  .stale.failed { color: var(--danger); }

  .retry {
    min-height: 82px;
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius);
    background: transparent;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 14px 14px 16px;
  }

  .ico {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    flex: none;
    border-radius: 12px;
    background: var(--warn-weak);
    color: var(--warn);
  }

  .txt {
    flex: 1;
    min-width: 0;
    display: grid;
    gap: 1px;
  }

  .txt strong {
    font-size: 14.5px;
    font-weight: 700;
  }

  .txt span {
    font-size: 13px;
    color: var(--text-3);
  }

  .again {
    min-height: 38px;
    padding: 0 12px;
    font-size: 13.5px;
  }

  .again span {
    display: grid;
  }
</style>
