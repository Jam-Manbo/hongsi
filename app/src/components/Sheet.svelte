<script lang="ts">
  import type { Snippet } from 'svelte';
  import { MediaQuery } from 'svelte/reactivity';
  import { fade, fly } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import { verticalDrag } from '../lib/vertical-drag';

  let {
    open = $bindable(false),
    title = '',
    titleMeta = '',
    wide = false,
    onclose,
    children,
    footer,
  }: {
    open: boolean;
    title?: string;
    titleMeta?: string;
    wide?: boolean;
    onclose?: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  const phone = new MediaQuery('max-width: 639px');
  const reducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)');
  const motion = $derived(reducedMotion.current ? { y: 0, duration: 0 } : phone.current ? { y: '100%', opacity: 1, duration: 260 } : { y: 24, opacity: 0, duration: 200 });

  let panel: HTMLDivElement | undefined = $state();
  let dragY = $state(0);
  let dragging = $state(false);

  function resetDrag() { dragging = false; dragY = 0; }
  function canDrag(target: HTMLElement) {
    if (!phone.current || target.closest('button, a, input, textarea, select, [contenteditable="true"], [role="slider"]')) return false;
    for (let node: HTMLElement | null = target; node && node !== panel; node = node.parentElement) {
      if (node.scrollTop > 0) return false;
    }
    return true;
  }
  function releaseDrag(distance: number, velocity: number) {
    dragging = false;
    const threshold = Math.min(140, (panel?.clientHeight ?? 500) * 0.25);
    if (distance >= threshold || (distance > 32 && velocity > 0.65)) close();
    else resetDrag();
  }

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function close() {
    open = false;
    onclose?.();
  }

  function onkeydown(e: KeyboardEvent) {
    if (open && e.key === 'Escape') close();
  }

  $effect(() => {
    if (open) {
      resetDrag();
      const prev = document.activeElement as HTMLElement | null;
      queueMicrotask(() => panel?.focus());
      document.body.style.overflow = 'hidden';
      return () => {
        document.body.style.overflow = '';
        prev?.focus?.();
      };
    }
  });
</script>

<svelte:window {onkeydown} />

{#if open}
  <div class="backdrop" use:portal transition:fade={{ duration: 160 }} onclick={close} aria-hidden="true"></div>
  <div
    class="sheet"
    class:wide
    class:dragging
    style:translate={phone.current ? `0 ${dragY}px` : undefined}
    use:portal
    use:verticalDrag={{ canStart: canDrag, move: (distance) => { dragging = true; dragY = distance; }, end: releaseDrag, cancel: resetDrag }}
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex="-1"
    bind:this={panel}
    transition:fly={motion}
  >
    <div class="grip" aria-hidden="true"></div>
    <header>
      <h2 class:with-meta={!!titleMeta}>
        {#if titleMeta}<span>{title}</span><span class="title-meta">{titleMeta}</span>{:else}{title}{/if}
      </h2>
      <button class="icon-btn" onclick={close} aria-label="닫기"><Icon name="close" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgb(12 18 27 / 42%);
    z-index: 60;
  }

  .sheet {
    position: fixed;
    z-index: 61;
    left: 0;
    right: 0;
    bottom: 0;
    max-height: 90dvh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    padding-bottom: var(--safe-b);
    outline: none;
    transition: translate 180ms var(--ease);
  }

  .sheet.dragging { transition: none; user-select: none; }

  .grip {
    width: 40px;
    height: 4px;
    border-radius: 4px;
    background: var(--border-strong);
    margin: 10px auto 2px;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px 10px 20px;
    flex: none;
  }

  h2 {
    font-size: 18px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }

  h2.with-meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .title-meta {
    flex: none;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-3);
  }

  .body {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior-y: contain;
    padding: 4px 20px 24px;
  }

  footer {
    display: flex;
    gap: 8px;
    padding: 14px 20px 16px;
    background: var(--surface);
    flex: none;
    border-top: 1px solid var(--border);
  }

  .sheet.wide {
    max-height: 92vh;
    max-height: 92dvh;
  }

  .sheet.wide .body {
    padding: 2px 10px 14px;
  }

  @media (min-width: 640px) {
    .sheet {
      left: 50%;
      right: auto;
      top: 50%;
      bottom: auto;
      width: min(560px, 92vw);
      transform: translate(-50%, -50%);
      border-radius: var(--radius-lg);
      max-height: 84vh;
    }

    .grip {
      display: none;
    }

    header {
      padding-top: 16px;
    }

    .sheet.wide {
      width: min(880px, 94vw);
      max-height: 90vh;
    }

    .sheet.wide .body {
      padding: 4px 16px 18px;
    }
  }
</style>
