<script lang="ts">
  import { WEEKDAYS } from '../lib/format';
  import Icon from './Icon.svelte';

  let { value = $bindable(''), label = '날짜' }: { value: string; label?: string } = $props();

  let input: HTMLInputElement | undefined = $state();

  const shown = $derived.by(() => {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
    const [y, m, d] = value.split('-').map(Number);
    return { main: `${y}년 ${m}월 ${d}일`, wd: WEEKDAYS[new Date(Date.UTC(y, m - 1, d)).getUTCDay()] };
  });

  function openPicker() {
    try {
      input?.showPicker?.();
    } catch {
    }
  }
</script>

<div class="date" class:empty={!shown}>
  <span class="shown" aria-hidden="true">
    <Icon name="calendar" size={18} />
    {#if shown}
      <span class="main">{shown.main}</span><span class="wd">({shown.wd})</span>
    {:else}
      <span class="main">날짜 없음</span>
    {/if}
  </span>
  <input bind:this={input} type="date" bind:value aria-label={label} onclick={openPicker} />
  {#if shown}
    <button type="button" class="clear" onclick={() => (value = '')} aria-label="날짜 지우기"><Icon name="close" size={16} /></button>
  {/if}
</div>

<style>
  .date {
    position: relative;
    height: 46px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }

  .date:focus-within {
    border-color: var(--primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent);
  }

  .shown {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 44px 0 12px;
    color: var(--text-3);
    pointer-events: none;
    white-space: nowrap;
    overflow: hidden;
  }

  .main {
    font-size: 15px;
    font-weight: 650;
    color: var(--text);
    font-variant-numeric: tabular-nums;
  }

  .empty .main {
    font-weight: 550;
    color: var(--text-3);
  }

  .wd {
    font-size: 14px;
    color: var(--text-3);
  }

  input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    cursor: pointer;
    border: 0;
    padding: 0;
  }

  input::-webkit-calendar-picker-indicator {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }

  .clear {
    position: absolute;
    right: 6px;
    top: 50%;
    z-index: 1;
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border-radius: 9px;
    color: var(--text-3);
  }

  .clear:hover {
    background: var(--surface-3);
    color: var(--text);
  }
</style>
