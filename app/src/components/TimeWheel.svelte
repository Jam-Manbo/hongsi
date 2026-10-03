<script lang="ts">
  import { onDestroy, untrack } from 'svelte';

  let { value = $bindable('09:00'), minuteStep = 5 }: { value: string; minuteStep?: 1 | 5 } = $props();

  type Column = 'h' | 'm';
  const ITEM = 40;
  const hours = Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0'));
  const minutes = $derived(Array.from({ length: 60 / minuteStep }, (_, i) => String(i * minuteStep).padStart(2, '0')));
  const hour = $derived(value.slice(0, 2));
  const minute = $derived(value.slice(3, 5));

  let hourEl: HTMLDivElement | undefined = $state();
  let minEl: HTMLDivElement | undefined = $state();
  const timers: Partial<Record<Column, ReturnType<typeof setTimeout>>> = {};
  const targets: Partial<Record<Column, number>> = {};
  let writtenValue = '';
  let appliedStep = 0;
  let placedHour: HTMLDivElement | undefined;
  let placedMinute: HTMLDivElement | undefined;
  let placementFrame = 0;
  let destroyed = false;

  function clearTimers() {
    cancelAnimationFrame(placementFrame);
    clearTimeout(timers.h);
    clearTimeout(timers.m);
  }
  onDestroy(() => { destroyed = true; clearTimers(); });

  function inactive(el: HTMLElement | undefined) {
    return destroyed || !el || !el.isConnected || !!el.closest('[inert]');
  }

  $effect(() => {
    const incoming = value, step = minuteStep, h = hourEl, m = minEl;
    untrack(() => {
      if (incoming === writtenValue && step === appliedStep && h === placedHour && m === placedMinute) return;
      clearTimers();
      const [nextHour, rawMinute] = (incoming || '09:00').split(':');
      const nextMinute = String(Math.min(60 - step, Math.round(Number(rawMinute) / step) * step)).padStart(2, '0');
      writtenValue = `${nextHour}:${nextMinute}`;
      value = writtenValue;
      appliedStep = step;
      placedHour = h;
      placedMinute = m;
      targets.h = hours.indexOf(nextHour);
      targets.m = minutes.indexOf(nextMinute);
      const place = () => {
        if (inactive(h) || inactive(m)) return;
        if (targets.h !== undefined) h?.scrollTo({ top: targets.h * ITEM, behavior: 'instant' });
        if (targets.m !== undefined) m?.scrollTo({ top: targets.m * ITEM, behavior: 'instant' });
      };
      place();
      placementFrame = requestAnimationFrame(place);
    });
  });

  function select(kind: Column, i: number) {
    if (inactive(kind === 'h' ? hourEl : minEl)) return;
    writtenValue = kind === 'h' ? `${hours[i]}:${minute}` : `${hour}:${minutes[i]}`;
    value = writtenValue;
  }

  function selectedIndex(kind: Column, el: HTMLDivElement) {
    const list = kind === 'h' ? hours : minutes;
    return Math.max(0, Math.min(list.length - 1, Math.round(el.scrollTop / ITEM)));
  }

  function settle(kind: Column) {
    const el = kind === 'h' ? hourEl : minEl;
    if (!el || inactive(el)) return;
    if (targets[kind] === undefined) select(kind, selectedIndex(kind, el));
    clearTimeout(timers[kind]);
    timers[kind] = setTimeout(() => {
      if (inactive(el)) return;
      const i = targets[kind] ?? selectedIndex(kind, el);
      delete targets[kind];
      select(kind, i);
      el.scrollTo({ top: i * ITEM, behavior: 'smooth' });
    }, 90);
  }

  function pick(kind: Column, i: number) {
    const el = kind === 'h' ? hourEl : minEl;
    if (!el || inactive(el)) return;
    targets[kind] = i;
    select(kind, i);
    el.scrollTo({ top: i * ITEM, behavior: 'smooth' });
    settle(kind);
  }

  function interact(kind: Column) {
    delete targets[kind];
  }

  function keyInteract(kind: Column, event: KeyboardEvent) {
    if (['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End'].includes(event.key)) interact(kind);
  }
</script>

<div class="wheel" style:--item="{ITEM}px">
  <div class="band" aria-hidden="true"></div>
  <div class="col" bind:this={hourEl} onscroll={() => settle('h')} onpointerdown={() => interact('h')} onwheel={() => interact('h')} onkeydown={(event) => keyInteract('h', event)} role="listbox" aria-label="시" tabindex="0">
    <div class="pad"></div>
    {#each hours as h, i (h)}
      <button type="button" class="it" class:on={h === hour} role="option" aria-selected={h === hour} onclick={() => pick('h', i)}>{h}</button>
    {/each}
    <div class="pad"></div>
  </div>
  <span class="colon">:</span>
  <div class="col" bind:this={minEl} onscroll={() => settle('m')} onpointerdown={() => interact('m')} onwheel={() => interact('m')} onkeydown={(event) => keyInteract('m', event)} role="listbox" aria-label="분" tabindex="0">
    <div class="pad"></div>
    {#each minutes as m, i (m)}
      <button type="button" class="it" class:on={m === minute} role="option" aria-selected={m === minute} onclick={() => pick('m', i)}>{m}</button>
    {/each}
    <div class="pad"></div>
  </div>
</div>

<style>
  .wheel {
    position: relative;
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
    height: calc(var(--item) * 5);
    border-radius: 16px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    overflow: hidden;
    -webkit-mask-image: linear-gradient(transparent, #000 30%, #000 70%, transparent);
    mask-image: linear-gradient(transparent, #000 30%, #000 70%, transparent);
  }

  .band {
    position: absolute;
    left: 12px;
    right: 12px;
    top: 50%;
    height: var(--item);
    transform: translateY(-50%);
    border-radius: 10px;
    background: var(--primary-weak);
    pointer-events: none;
  }

  .col {
    position: relative;
    width: 72px;
    height: 100%;
    overflow-y: auto;
    scroll-snap-type: y mandatory;
    scrollbar-width: none;
    overscroll-behavior: contain;
  }

  .col::-webkit-scrollbar {
    display: none;
  }

  .pad {
    height: calc(var(--item) * 2);
  }

  .it {
    display: grid;
    place-items: center;
    width: 100%;
    height: var(--item);
    scroll-snap-align: center;
    font-size: 20px;
    font-weight: 600;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .it.on {
    color: var(--primary-text);
    font-weight: 800;
    font-size: 22px;
  }

  .colon {
    position: relative;
    font-size: 22px;
    font-weight: 800;
    color: var(--primary-text);
  }
</style>
