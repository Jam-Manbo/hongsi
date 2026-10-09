import { untrack } from 'svelte';
import { MediaQuery } from 'svelte/reactivity';

export function createCalendarCollapse(enabled: () => boolean) {
  let calWrap: HTMLDivElement | undefined = $state();
  let calSpace: HTMLDivElement | undefined = $state();
  let monthView: HTMLDivElement | undefined = $state();
  let weekView: HTMLDivElement | undefined = $state();
  let dayTitle: HTMLHeadingElement | undefined = $state();
  let monthH = $state(0);
  let stripH = $state(0);
  let collapse = $state(0);
  const stripOn = $derived(collapse >= 0.999);
  const reducedMotion = new MediaQuery('(prefers-reduced-motion: reduce)');
  const compactInteractive = $derived(collapse >= (reducedMotion.current ? 0.999 : 0.85));

  $effect(() => {
    if (!enabled() || !calWrap || !calSpace || !monthView || !weekView) {
      collapse = 0;
      return;
    }
    const space = calSpace, full = monthView, compact = weekView;
    const scroller = calWrap.closest('.scroller');
    if (!scroller) return;
    let raf = 0;
    let previousHeight = 0;
    const check = () => {
      raf = 0;
      const bar = document.querySelector('.topbar');
      const edge = (bar ?? scroller).getBoundingClientRect()[bar ? 'bottom' : 'top'];
      collapse = Math.max(0, Math.min(1, (edge - space.getBoundingClientRect().top) / Math.max(1, monthH - stripH)));
    };
    const onScroll = () => {
      if (!raf) raf = requestAnimationFrame(check);
    };
    const measure = () => {
      const height = full.offsetHeight;
      const adjustment = previousHeight && stripOn ? height - previousHeight : 0;
      previousHeight = height;
      monthH = height;
      stripH = compact.offsetHeight;
      if (adjustment) scroller.scrollTop += adjustment;
      onScroll();
    };
    const observer = new ResizeObserver(measure);
    observer.observe(full);
    observer.observe(compact);
    scroller.addEventListener('scroll', onScroll, { passive: true });
    untrack(measure);
    return () => {
      observer.disconnect();
      scroller.removeEventListener('scroll', onScroll);
      cancelAnimationFrame(raf);
    };
  });

  function afterStripPick() {
    requestAnimationFrame(() => dayTitle?.scrollIntoView({ block: 'start', behavior: 'smooth' }));
  }

  function expand() {
    (calSpace ?? calWrap)?.scrollIntoView({ block: 'start', behavior: 'smooth' });
  }

  return {
    get calWrap() { return calWrap; },
    set calWrap(value: typeof calWrap) { calWrap = value; },
    get calSpace() { return calSpace; },
    set calSpace(value: typeof calSpace) { calSpace = value; },
    get monthView() { return monthView; },
    set monthView(value: typeof monthView) { monthView = value; },
    get weekView() { return weekView; },
    set weekView(value: typeof weekView) { weekView = value; },
    get dayTitle() { return dayTitle; },
    set dayTitle(value: typeof dayTitle) { dayTitle = value; },
    get monthH() { return monthH; },
    get stripH() { return stripH; },
    get collapse() { return collapse; },
    get stripOn() { return stripOn; },
    get compactInteractive() { return compactInteractive; },
    afterStripPick,
    expand,
  };
}
