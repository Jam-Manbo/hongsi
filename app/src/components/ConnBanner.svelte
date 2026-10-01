<script lang="ts">
  import { slide } from 'svelte/transition';
  import { net, retry, trouble, TROUBLE_TEXT } from '../lib/net.svelte';
  import Icon from './Icon.svelte';

  const t = $derived(trouble());
  const ICON = { offline: 'wifi-off', server: 'cloud-off', school: 'alert' } as const;
</script>

{#if t}
  <div class="banner {t}" role="status" transition:slide={{ duration: 180 }}>
    <span class="ico"><Icon name={ICON[t]} size={17} stroke={2} /></span>
    <p>
      <strong>{TROUBLE_TEXT[t].title}</strong>
    </p>
    <button class="again" onclick={() => retry()} disabled={net.checking} aria-label="연결 다시 확인">
      <span class:spin={net.checking}><Icon name="refresh" size={15} stroke={2.3} /></span>
      <span class="label">{net.checking ? '확인 중' : '다시 시도'}</span>
    </button>
  </div>
{/if}

<style>
  .banner {
    flex: 1 0 100%;
    order: 3;
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 8px;
    padding: 7px 8px 7px 12px;
    border-radius: 12px;
    background: var(--warn-weak);
    color: var(--warn);
    border: 1px solid color-mix(in srgb, var(--warn) 22%, transparent);
  }

  .banner.offline {
    background: var(--surface-3);
    color: var(--text-2);
    border-color: var(--border);
  }

  .ico {
    display: grid;
    flex: none;
  }

  p {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 0 8px;
    font-size: 13px;
    line-height: 1.4;
  }

  strong {
    font-weight: 750;
  }

  .again {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 30px;
    padding: 0 10px;
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font-size: 12.5px;
    font-weight: 700;
    box-shadow: var(--shadow-sm);
  }

  .again:disabled {
    cursor: default;
    opacity: 0.8;
  }

  .again span {
    display: grid;
  }

  @media (max-width: 639px) {
    .banner {
      margin: 8px 4px 0 -4px;
    }

    p {
      display: grid;
      gap: 0;
      font-size: 12.5px;
    }

  }

  @media (min-width: 640px) {
    .banner {
      margin-right: 8px;
    }
  }
</style>
