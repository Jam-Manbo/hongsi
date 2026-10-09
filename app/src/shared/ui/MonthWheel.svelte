<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { todayKey } from '../utils/format';

  let { year = $bindable(), month = $bindable() }: { year: number; month: number } = $props();

  type Column = 'year' | 'month';
  const ITEM = 40;
  const firstYear = 2000;
  const lastYear = Number(todayKey().slice(0, 4));
  const years = Array.from({ length: lastYear - firstYear + 1 }, (_, i) => firstYear + i);
  const months = Array.from({ length: 12 }, (_, i) => i + 1);
  let yearEl: HTMLDivElement | undefined = $state();
  let monthEl: HTMLDivElement | undefined = $state();
  const timers: Partial<Record<Column, ReturnType<typeof setTimeout>>> = {};
  const targets: Partial<Record<Column, number>> = {};
  let writtenYear = 0;
  let writtenMonth = 0;
  let placedYear: HTMLDivElement | undefined;
  let placedMonth: HTMLDivElement | undefined;
  let placementFrame = 0;
  let destroyed = false;

  function clearTimers() {
    cancelAnimationFrame(placementFrame);
    clearTimeout(timers.year);
    clearTimeout(timers.month);
  }
  onDestroy(() => { destroyed = true; clearTimers(); });

  function inactive(el: HTMLElement | undefined) {
    return destroyed || !el || !el.isConnected || !!el.closest('[inert]');
  }

  $effect(() => {
    const incomingYear = year, incomingMonth = month, y = yearEl, m = monthEl;
    untrack(() => {
      if (incomingYear === writtenYear && incomingMonth === writtenMonth && y === placedYear && m === placedMonth) return;
      clearTimers();
      const selectedYear = Math.max(firstYear, Math.min(lastYear, incomingYear));
      year = writtenYear = selectedYear;
      writtenMonth = incomingMonth;
      placedYear = y;
      placedMonth = m;
      targets.year = years.indexOf(selectedYear);
      targets.month = months.indexOf(incomingMonth);
      const place = () => {
        if (inactive(y) || inactive(m)) return;
        if (targets.year !== undefined) y?.scrollTo({ top: targets.year * ITEM, behavior: 'instant' });
        if (targets.month !== undefined) m?.scrollTo({ top: targets.month * ITEM, behavior: 'instant' });
      };
      place();
      placementFrame = requestAnimationFrame(place);
    });
  });

  function select(kind: Column, i: number) {
    if (inactive(kind === 'year' ? yearEl : monthEl)) return;
    if (kind === 'year') year = writtenYear = years[i];
    else month = writtenMonth = months[i];
  }

  function selectedIndex(kind: Column, el: HTMLDivElement) {
    const list = kind === 'year' ? years : months;
    return Math.max(0, Math.min(list.length - 1, Math.round(el.scrollTop / ITEM)));
  }

  function settle(kind: Column) {
    const el = kind === 'year' ? yearEl : monthEl;
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
    const el = kind === 'year' ? yearEl : monthEl;
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
  <div class="col year" bind:this={yearEl} onscroll={() => settle('year')} onpointerdown={() => interact('year')} onwheel={() => interact('year')} onkeydown={(event) => keyInteract('year', event)} role="listbox" aria-label="연도" tabindex="0">
    <div class="pad"></div>
    {#each years as value, i (value)}
      <button type="button" class="it" class:on={value === year} role="option" aria-selected={value === year} onclick={() => pick('year', i)}>{value}년</button>
    {/each}
    <div class="pad"></div>
  </div>
  <div class="col" bind:this={monthEl} onscroll={() => settle('month')} onpointerdown={() => interact('month')} onwheel={() => interact('month')} onkeydown={(event) => keyInteract('month', event)} role="listbox" aria-label="월" tabindex="0">
    <div class="pad"></div>
    {#each months as value, i (value)}
      <button type="button" class="it" class:on={value === month} role="option" aria-selected={value === month} onclick={() => pick('month', i)}>{value}월</button>
    {/each}
    <div class="pad"></div>
  </div>
</div>

<style>
  .wheel { position: relative; display: flex; justify-content: center; align-items: center; gap: 24px; height: calc(var(--item) * 5); border-radius: 16px; background: var(--surface-2); border: 1px solid var(--border); overflow: hidden; -webkit-mask-image: linear-gradient(transparent, #000 30%, #000 70%, transparent); mask-image: linear-gradient(transparent, #000 30%, #000 70%, transparent); }
  .band { position: absolute; left: 12px; right: 12px; top: 50%; height: var(--item); transform: translateY(-50%); border-radius: 10px; background: var(--primary-weak); pointer-events: none; }
  .col { position: relative; width: 72px; height: 100%; overflow-y: auto; scroll-snap-type: y mandatory; scrollbar-width: none; overscroll-behavior: contain; }
  .col.year { width: 112px; }
  .col::-webkit-scrollbar { display: none; }
  .pad { height: calc(var(--item) * 2); }
  .it { display: grid; place-items: center; width: 100%; height: var(--item); scroll-snap-align: center; font-size: 20px; font-weight: 600; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .it.on { color: var(--primary-text); font-weight: 800; font-size: 22px; }
</style>
