<script module lang="ts">
  export { closeSheets } from '../utils/modal-stack';
</script>

<script lang="ts">
  import { untrack, type Snippet } from 'svelte';
  import { isTopSheet, registerSheet } from '../utils/modal-stack';
  import { MediaQuery } from 'svelte/reactivity';
  import { fade, fly } from 'svelte/transition';
  import Icon from './Icon.svelte';
  import { verticalDrag } from '../utils/vertical-drag';

  let {
    open = $bindable(false),
    title = '',
    titleMeta = '',
    titleIcon = '',
    wide = false,
    confirm = false,
    showClose = true,
    closeDisabled = false,
    onbeforeclose,
    onclose,
    children,
    headerActions,
    footer,
  }: {
    open: boolean;
    title?: string;
    titleMeta?: string;
    titleIcon?: string;
    wide?: boolean;
    confirm?: boolean;
    showClose?: boolean;
    closeDisabled?: boolean;
    onbeforeclose?: () => boolean;
    onclose?: () => void;
    children?: Snippet;
    headerActions?: Snippet;
    footer?: Snippet;
  } = $props();

  const phone = new MediaQuery('max-width: 639px');
  const reducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)');
  const motion = $derived(reducedMotion.current ? { y: 0, duration: 0 } : phone.current && !confirm ? { y: '100%', opacity: 1, duration: 260 } : { y: 24, opacity: 0, duration: 200 });

  let panel: HTMLDivElement | undefined = $state();
  let backdrop: HTMLDivElement | undefined = $state();
  let body: HTMLDivElement | undefined = $state();
  let bodyContent: HTMLDivElement | undefined = $state();
  let dragY = $state(0);
  let dragging = $state(false);

  function resetDrag() { dragging = false; dragY = 0; }
  function canDrag(target: HTMLElement) {
    if (!phone.current || confirm || !isTopSheet(panel)) return false;
    if (target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="slider"], [role="listbox"]')) return false;
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
    if (onbeforeclose?.() === false) {
      resetDrag();
      return false;
    }
    open = false;
    onclose?.();
    return true;
  }

  $effect(() => {
    if (!open || !panel || !backdrop) return;
    const currentPanel = panel, currentBackdrop = backdrop;
    return untrack(() => {
      resetDrag();
      return registerSheet(currentPanel, currentBackdrop, close);
    });
  });

  $effect(() => {
    if (!open || !body || !bodyContent || !footer || wide || confirm) return;
    const container = body, content = bodyContent;
    let frame = 0;
    const measure = () => {
      frame = 0;
      container.style.removeProperty('--body-bottom-space');
      const style = getComputedStyle(container);
      const preferred = parseFloat(style.getPropertyValue('--body-bottom'));
      const remaining = container.getBoundingClientRect().height - parseFloat(style.paddingTop)
        - parseFloat(style.borderTopWidth) - parseFloat(style.borderBottomWidth) - content.getBoundingClientRect().height;
      const space = remaining >= -1 ? Math.max(0, Math.min(preferred, remaining)) : preferred;
      container.style.setProperty('--body-bottom-space', `${space}px`);
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(measure); };
    const observer = new ResizeObserver(schedule);
    observer.observe(container);
    observer.observe(content);
    window.addEventListener('resize', schedule);
    window.visualViewport?.addEventListener('resize', schedule);
    measure();
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      window.removeEventListener('resize', schedule);
      window.visualViewport?.removeEventListener('resize', schedule);
      container.style.removeProperty('--body-bottom-space');
    };
  });
</script>

{#if open}
  <div class="backdrop" bind:this={backdrop} use:portal transition:fade={{ duration: reducedMotion.current ? 0 : 160 }} onclick={() => { if (isTopSheet(panel)) close(); }} aria-hidden="true"></div>
  <div
    class="sheet"
    class:wide
    class:confirm
    class:message={!children}
    class:dragging
    style:translate={phone.current && !confirm ? `0 ${dragY}px` : undefined}
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
    <header class:has-actions={!!headerActions}>
      <h2 class:with-meta={!!titleMeta} class:with-icon={!!titleIcon}>
        {#if titleIcon}<span class="title-icon"><Icon name={titleIcon} size={21} /></span>{/if}
        {#if titleMeta}<span>{title}</span><span class="title-meta">{titleMeta}</span>{:else}{title}{/if}
      </h2>
      {#if headerActions}{@render headerActions()}{/if}
      {#if showClose}<button class="icon-btn" disabled={closeDisabled} onclick={close} aria-label="닫기"><Icon name="close" /></button>{/if}
    </header>
    {#if children}<div class="body" bind:this={body}><div class="body-content" bind:this={bodyContent}>{@render children()}</div></div>{/if}
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: var(--modal-backdrop);
    touch-action: none;
    z-index: 60;
  }

  .backdrop:global([data-covered]) { visibility: hidden; }

  .sheet {
    --sheet-surface: var(--modal-surface);
    position: fixed;
    z-index: 61;
    left: 0;
    right: 0;
    bottom: 0;
    max-height: 90dvh;
    display: flex;
    flex-direction: column;
    background: var(--sheet-surface);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    border: 1px solid var(--modal-border);
    box-shadow: var(--modal-shadow);
    overflow: hidden;
    padding-bottom: var(--safe-b);
    outline: none;
    transition: translate 180ms var(--ease);
  }

  .sheet:global([data-covered]) { pointer-events: none; }

  .sheet.dragging { transition: none; user-select: none; }

  .grip {
    flex: none;
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

  header.has-actions h2 { margin-right: auto; }

  .sheet.message header { padding: 24px 20px; }

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

  h2.with-icon {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .title-icon {
    display: flex;
    flex: none;
    color: var(--primary);
  }

  .title-meta {
    flex: none;
    white-space: nowrap;
    font-size: 14px;
    font-weight: 500;
    color: var(--text-3);
  }

  .body {
    --body-bottom: 24px;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior-y: none;
    padding: 4px 20px var(--body-bottom-space, var(--body-bottom));
    transition: none;
  }

  .body:has(+ footer) { --body-bottom: 16px; }

  .body-content { display: flow-root; }
  .body-content > :global(:last-child) { margin-bottom: 0; }

  footer {
    display: flex;
    gap: 8px;
    padding: 14px 20px 16px;
    background: inherit;
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
  .sheet.confirm {
    inset: 50% auto auto 50%;
    transform: translate(-50%, -50%);
    width: min(400px, calc(100vw - 48px));
    max-height: min(84dvh, calc(100dvh - env(safe-area-inset-top, 0px) - var(--safe-b) - 48px));
    border-radius: 22px;
    padding-bottom: 0;
  }

  .confirm .grip { display: none; }
  .confirm header { padding: 22px 20px 14px; align-items: flex-start; }
  .confirm h2 { font-size: 18px; line-height: 1.45; }
  .confirm .body { padding: 0 20px 22px; }
  .confirm footer { padding: 16px 20px 20px; }

</style>
