<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    value,
    size = 120,
    stroke = 10,
    color = 'var(--primary)',
    children,
  }: { value: number; size?: number; stroke?: number; color?: string; children?: Snippet } = $props();

  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
  const offset = $derived(c * (1 - Math.min(1, Math.max(0, value))));
</script>

<div class="ring" style:width="{size}px" style:height="{size}px">
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    <circle cx={size / 2} cy={size / 2} {r} stroke="var(--surface-3)" stroke-width={stroke} fill="none" />
    <circle
      cx={size / 2}
      cy={size / 2}
      {r}
      stroke={color}
      stroke-width={stroke}
      fill="none"
      stroke-linecap="round"
      stroke-dasharray={c}
      stroke-dashoffset={offset}
      transform="rotate(-90 {size / 2} {size / 2})"
    />
  </svg>
  <div class="center">{@render children?.()}</div>
</div>

<style>
  .ring {
    position: relative;
    flex: none;
  }

  circle {
    transition:
      stroke-dashoffset 0.6s var(--ease),
      stroke 0.3s;
  }

  .center {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    text-align: center;
  }
</style>
