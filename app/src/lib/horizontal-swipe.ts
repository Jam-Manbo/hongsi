import { tick } from 'svelte';

type Options = {
  enabled: () => boolean;
  shift: (direction: -1 | 1) => void;
};

export function horizontalSwipe(node: HTMLElement, options: Options) {
  let start: { id: number; x: number; y: number } | null = null;
  let dragging = false, settling = false, destroyed = false;
  let distance = 0, width = 0, lastX = 0, lastAt = 0, velocity = 0, suppressUntil = 0;
  let frame = 0;
  let track: HTMLElement | null = null;
  let animation: Animation | null = null;
  const reducedMotion = matchMedia('(prefers-reduced-motion: reduce)');

  function paint() {
    frame = 0;
    if (track) track.style.transform = `translate3d(${distance}px, 0, 0)`;
  }

  function resetPointer() {
    const id = start?.id;
    start = null;
    dragging = false;
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    if (id !== undefined && node.hasPointerCapture(id)) node.releasePointerCapture(id);
  }

  async function settle(direction: -1 | 0 | 1) {
    if (!track || settling) return;
    settling = true;
    paint();
    const target = direction === 0 ? 0 : -direction * width;
    const duration = reducedMotion.matches ? 0 : Math.min(260, Math.max(140, Math.abs(target - distance) * 0.7));
    animation = track.animate([
      { transform: `translate3d(${distance}px, 0, 0)` },
      { transform: `translate3d(${target}px, 0, 0)` },
    ], { duration, easing: 'cubic-bezier(.22,.7,.2,1)', fill: 'forwards' });
    try {
      await animation.finished;
      if (destroyed) return;
      if (direction && options.enabled()) {
        options.shift(direction);
        await tick();
      }
    } catch {
    } finally {
      animation?.cancel();
      animation = null;
      distance = 0;
      if (track) { track.style.transform = ''; track.style.willChange = ''; }
      settling = false;
    }
  }

  function begin(event: PointerEvent) {
    if (!event.isPrimary) { if (start) cancel(); return; }
    if (settling || !options.enabled() || event.pointerType === 'mouse') return;
    resetPointer();
    track = node.querySelector<HTMLElement>('.calendar-pages');
    if (!track) return;
    width = track.clientWidth;
    suppressUntil = 0;
    distance = 0;
    lastX = event.clientX; lastAt = performance.now(); velocity = 0;
    start = { id: event.pointerId, x: event.clientX, y: event.clientY };
  }

  function move(event: PointerEvent) {
    if (!start || event.pointerId !== start.id) return;
    if (!options.enabled()) { cancel(); return; }
    const dx = event.clientX - start.x, dy = event.clientY - start.y;
    if (!dragging) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < 8) return;
      if (Math.abs(dx) < Math.abs(dy) * 1.2) { resetPointer(); return; }
      dragging = true;
      node.setPointerCapture(event.pointerId);
      if (track) track.style.willChange = 'transform';
    }
    const now = performance.now();
    velocity = (event.clientX - lastX) / Math.max(1, now - lastAt);
    lastX = event.clientX; lastAt = now;
    distance = Math.max(-width, Math.min(width, dx));
    if (!frame) frame = requestAnimationFrame(paint);
    suppressUntil = now + 400;
  }

  function end(event: PointerEvent) {
    if (!start || event.pointerId !== start.id) return;
    if (!dragging) { resetPointer(); return; }
    const threshold = Math.min(90, width * 0.24);
    const flick = performance.now() - lastAt < 100 && Math.abs(velocity) > 0.45 && Math.abs(distance) > 24 && Math.sign(velocity) === Math.sign(distance);
    const direction = options.enabled() && (Math.abs(distance) >= threshold || flick) ? (distance < 0 ? 1 : -1) : 0;
    suppressUntil = performance.now() + 400;
    resetPointer();
    void settle(direction);
  }

  function cancel() {
    const moved = dragging;
    resetPointer();
    if (moved) { suppressUntil = performance.now() + 400; void settle(0); }
  }

  function cancelled(event: PointerEvent) {
    if (event.pointerId === start?.id) cancel();
  }

  function lostCapture(event: PointerEvent) {
    if (event.target === node) cancelled(event);
  }

  function click(event: MouseEvent) {
    if (event.detail > 0 && (settling || performance.now() < suppressUntil)) {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  node.addEventListener('pointerdown', begin);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', end);
  node.addEventListener('pointercancel', cancelled);
  node.addEventListener('lostpointercapture', lostCapture);
  node.addEventListener('click', click, true);

  return {
    update(next: Options) { options = next; },
    destroy() {
      destroyed = true;
      resetPointer();
      animation?.cancel();
      node.removeEventListener('pointerdown', begin);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', end);
      node.removeEventListener('pointercancel', cancelled);
      node.removeEventListener('lostpointercapture', lostCapture);
      node.removeEventListener('click', click, true);
    },
  };
}
