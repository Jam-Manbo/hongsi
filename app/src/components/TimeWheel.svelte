<script lang="ts">
  let { value = $bindable('09:00') }: { value: string } = $props();

  const ITEM = 40;
  const hours = Array.from({ length: 24 }, (_, i) => String(i).padStart(2, '0'));
  const minutes = Array.from({ length: 12 }, (_, i) => String(i * 5).padStart(2, '0'));

  let hourEl: HTMLDivElement | undefined = $state();
  let minEl: HTMLDivElement | undefined = $state();
  let timers: Record<string, ReturnType<typeof setTimeout>> = {};

  const [h0, m0] = (value || '09:00').split(':');
  let hour = $state(h0);
  let minute = $state(String(Math.min(55, Math.round(Number(m0) / 5) * 5)).padStart(2, '0'));

  $effect(() => {
    value = `${hour}:${minute}`;
  });

  $effect(() => {
    hourEl?.scrollTo({ top: hours.indexOf(hour) * ITEM });
    minEl?.scrollTo({ top: minutes.indexOf(minute) * ITEM });
  });

  function settle(kind: 'h' | 'm') {
    clearTimeout(timers[kind]);
    timers[kind] = setTimeout(() => {
      const el = kind === 'h' ? hourEl : minEl;
      if (!el) return;
      const list = kind === 'h' ? hours : minutes;
      const i = Math.max(0, Math.min(list.length - 1, Math.round(el.scrollTop / ITEM)));
      el.scrollTo({ top: i * ITEM, behavior: 'smooth' });
      if (kind === 'h') hour = list[i];
      else minute = list[i];
    }, 90);
  }

  function pick(kind: 'h' | 'm', i: number) {
    const el = kind === 'h' ? hourEl : minEl;
    el?.scrollTo({ top: i * ITEM, behavior: 'smooth' });
  }
</script>

<div class="wheel" style:--item="{ITEM}px">
  <div class="band" aria-hidden="true"></div>
  <div class="col" bind:this={hourEl} onscroll={() => settle('h')} role="listbox" aria-label="시" tabindex="0">
    <div class="pad"></div>
    {#each hours as h, i (h)}
      <button type="button" class="it" class:on={h === hour} role="option" aria-selected={h === hour} onclick={() => pick('h', i)}>{h}</button>
    {/each}
    <div class="pad"></div>
  </div>
  <span class="colon">:</span>
  <div class="col" bind:this={minEl} onscroll={() => settle('m')} role="listbox" aria-label="분" tabindex="0">
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
