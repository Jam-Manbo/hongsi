<script lang="ts">
  import { tick, type Snippet } from 'svelte';

  type Side = 'top' | 'bottom' | 'left' | 'right';

  let {
    anchor,
    open = $bindable(false),
    placement = 'top',
    timeout = 0,
    label = '',
    role = 'dialog',
    onclose,
    children,
  }: {
    anchor: HTMLElement | null;
    open?: boolean;
    placement?: 'top' | 'bottom' | 'side';
    timeout?: number;
    label?: string;
    role?: 'dialog' | 'tooltip';
    onclose?: () => void;
    children: Snippet;
  } = $props();

  let el: HTMLDivElement | undefined = $state();
  let x = $state(0);
  let y = $state(0);
  let side = $state<Side>('top');
  let arrow = $state(0);
  let shown = $state(false);
  let hovering = false;

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  function close() {
    if (!open) return;
    open = false;
    onclose?.();
  }

  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));

  function place() {
    if (!anchor || !el) return;
    const a = anchor.getBoundingClientRect();
    const vw = document.documentElement.clientWidth;
    const vh = window.innerHeight;
    if (a.bottom < 0 || a.top > vh || (a.width === 0 && a.height === 0)) {
      close();
      return;
    }
    const w = el.offsetWidth;
    const h = el.offsetHeight;
    const m = 8;
    const gap = 8;
    let s: Side = placement === 'side' ? 'right' : placement;
    if (s === 'right' && a.right + gap + w > vw - m) s = a.left - gap - w >= m ? 'left' : 'bottom';
    if (s === 'top' && a.top - gap - h < m) s = 'bottom';
    if (s === 'bottom' && a.bottom + gap + h > vh - m && a.top - gap - h >= m) s = 'top';
    if (s === 'left' || s === 'right') {
      x = s === 'right' ? a.right + gap : a.left - gap - w;
      y = clamp(a.top - 6, m, Math.max(m, vh - h - m));
      arrow = clamp(a.top + Math.min(a.height / 2, 22) - y, 14, h - 14);
    } else {
      x = clamp(a.left + a.width / 2 - w / 2, m, Math.max(m, vw - w - m));
      y = s === 'top' ? a.top - gap - h : a.bottom + gap;
      arrow = clamp(a.left + a.width / 2 - x, 14, w - 14);
    }
    side = s;
  }

  $effect(() => {
    if (!open) {
      shown = false;
      return;
    }
    void anchor;
    let raf = 0;
    const update = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(place);
    };
    tick().then(() => {
      place();
      shown = true;
      if (role === 'dialog') el?.focus({ preventScroll: true });
    });
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (el?.contains(t) || anchor?.contains(t)) return;
      close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      close();
      anchor?.focus();
    };
    document.addEventListener('pointerdown', onDown, true);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('resize', update);
    document.addEventListener('scroll', update, true);
    return () => {
      cancelAnimationFrame(raf);
      document.removeEventListener('pointerdown', onDown, true);
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('resize', update);
      document.removeEventListener('scroll', update, true);
    };
  });

  let timer: ReturnType<typeof setTimeout> | undefined;
  function arm() {
    clearTimeout(timer);
    if (timeout > 0) timer = setTimeout(() => !hovering && close(), timeout);
  }
  $effect(() => {
    if (!open || timeout <= 0) return;
    void anchor;
    arm();
    return () => clearTimeout(timer);
  });
</script>

{#if open}
  <div
    class="pop {side}"
    class:shown
    class:tip={role === 'tooltip'}
    use:portal
    bind:this={el}
    {role}
    aria-label={label || undefined}
    tabindex="-1"
    style:left="{x}px"
    style:top="{y}px"
    style:--arrow="{arrow}px"
    onpointerenter={() => (hovering = true)}
    onpointerleave={() => {
      hovering = false;
      arm();
    }}
  >
    {@render children()}
  </div>
{/if}

<style>
  .pop {
    position: fixed;
    z-index: 75;
    max-width: min(340px, calc(100vw - 16px));
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    outline: none;
    opacity: 0;
    transform: translateY(4px) scale(0.98);
    transition:
      opacity 0.14s,
      transform 0.16s var(--ease);
  }

  .pop.bottom {
    transform: translateY(-4px) scale(0.98);
  }

  .pop.shown {
    opacity: 1;
    transform: none;
  }

  .pop.tip {
    padding: 9px 12px;
    border-radius: 12px;
    background: var(--text);
    color: var(--surface);
    border-color: transparent;
    pointer-events: none;
  }

  .pop::after {
    content: '';
    position: absolute;
    width: 10px;
    height: 10px;
    background: inherit;
    border: inherit;
    transform: rotate(45deg);
  }

  .pop.top::after {
    left: calc(var(--arrow) - 5px);
    bottom: -6px;
    border-top-color: transparent;
    border-left-color: transparent;
  }

  .pop.bottom::after {
    left: calc(var(--arrow) - 5px);
    top: -6px;
    border-bottom-color: transparent;
    border-right-color: transparent;
  }

  .pop.right::after {
    top: calc(var(--arrow) - 5px);
    left: -6px;
    border-top-color: transparent;
    border-right-color: transparent;
  }

  .pop.left::after {
    top: calc(var(--arrow) - 5px);
    right: -6px;
    border-bottom-color: transparent;
    border-left-color: transparent;
  }
</style>
