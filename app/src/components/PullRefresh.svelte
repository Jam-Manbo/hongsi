<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { Snippet } from 'svelte';
  import { verticalDrag } from '../lib/vertical-drag';
  import Icon from './Icon.svelte';

  let { onrefresh, refreshing, slow, pageKey, children }: {
    onrefresh: () => Promise<unknown>; refreshing: boolean; slow: boolean; pageKey: string; children: Snippet;
  } = $props();
  const THRESHOLD = 60;
  const displacement = (distance: number) => 96 * (1 - Math.exp(-distance / 120));
  let el: HTMLDivElement | undefined = $state();
  let pull = $state(0);
  let dragging = $state(false);
  let holding = $state(false);
  let generation = 0;
  let holdTimer: ReturnType<typeof setTimeout> | undefined;
  const busy = $derived(refreshing || holding);
  const ready = $derived(pull >= THRESHOLD);
  const visible = $derived(pull > 0 || holding);
  const progress = $derived(Math.min(1, pull / THRESHOLD));
  const position = $derived(holding ? 14 : -44 + pull);

  function reset() { dragging = false; pull = 0; }
  function canStart(target: HTMLElement) {
    if (busy || !el || el.scrollTop > 0 || target.closest('input, textarea, select, [contenteditable="true"], [role="slider"]')) return false;
    for (let node: HTMLElement | null = target; node && node !== el; node = node.parentElement) {
      if (node.scrollTop > 0) return false;
    }
    return true;
  }
  function release(distance: number) {
    const trigger = displacement(distance) >= THRESHOLD && !busy;
    reset();
    if (!trigger) return;
    const current = generation;
    const started = performance.now();
    holding = true;
    void Promise.resolve().then(onrefresh).catch(() => {}).finally(() => {
      if (current !== generation) return;
      holdTimer = setTimeout(() => { holding = false; }, Math.max(0, 320 - (performance.now() - started)));
    });
  }
  $effect(() => {
    pageKey;
    generation++;
    clearTimeout(holdTimer);
    holding = false;
    reset();
  });
  onDestroy(() => { generation++; clearTimeout(holdTimer); });
</script>

<div class="refresh-region">
  <div class="scroller" bind:this={el} use:verticalDrag={{
    canStart,
    move: (distance) => { dragging = true; pull = displacement(distance); },
    end: release,
    cancel: reset,
  }}>
    {@render children()}
  </div>
  <div class="pull-layer" aria-hidden="true">
  <div class="ptr" class:dragging class:visible class:ready
    style:transform="translate3d(-50%, {visible ? position : -44}px, 0)" aria-hidden="true">
    {#if busy}
      <svg class="spinning" width="23" height="23" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-dasharray="40 17" /></svg>
    {:else}
      <span style:transform="rotate({progress * 240}deg)"><Icon name="refresh" size={22} stroke={2} /></span>
    {/if}
  </div>
  </div>
  {#if busy}
    <span class="sr-only" role="status">{slow ? '학교 서버 응답을 기다리는 중' : '새로고침 중'}</span>
  {/if}
</div>

<style>
  .refresh-region { position: relative; height: 100%; min-height: 0; overflow: hidden; }
  .scroller { height: 100%; overflow-y: auto; overscroll-behavior-y: none; -webkit-overflow-scrolling: touch; }
  .pull-layer { position: absolute; inset: var(--topbar-h, 64px) 0 0; overflow: hidden; pointer-events: none; z-index: 40; }
  .ptr {
    position: absolute; top: 0; left: 50%;
    display: grid; place-items: center; width: 40px; height: 40px; border-radius: 50%;
    background: var(--surface); color: var(--primary); box-shadow: var(--shadow-sm); border: 1px solid var(--border);
    pointer-events: none; opacity: 0; will-change: transform;
    transition: transform 200ms cubic-bezier(.2,.8,.2,1), opacity 140ms;
  }
  .ptr.visible { opacity: 1; }
  .ptr.dragging { transition: none; }
  .spinning { animation: refresh-spin 850ms linear infinite; transform-origin: center; }
  @keyframes refresh-spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) {
    .ptr { transition: opacity 100ms; }
    .spinning { animation-duration: 1400ms; }
  }
</style>
