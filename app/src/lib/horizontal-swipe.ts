type Options = {
  enabled: () => boolean;
  shift: (direction: -1 | 1) => void;
};

export function horizontalSwipe(node: HTMLElement, options: Options) {
  let start: { id: number; x: number; y: number } | null = null;
  let dragging = false;
  let suppressUntil = 0;

  function reset() {
    const id = start?.id;
    start = null;
    dragging = false;
    if (id !== undefined && node.hasPointerCapture(id)) node.releasePointerCapture(id);
  }

  function begin(event: PointerEvent) {
    reset();
    if (!options.enabled() || !event.isPrimary || event.pointerType === 'mouse') return;
    suppressUntil = 0;
    start = { id: event.pointerId, x: event.clientX, y: event.clientY };
  }

  function move(event: PointerEvent) {
    if (!start || event.pointerId !== start.id) return;
    if (!options.enabled()) { reset(); return; }
    const dx = event.clientX - start.x;
    const dy = event.clientY - start.y;
    if (!dragging) {
      if (Math.max(Math.abs(dx), Math.abs(dy)) < 10) return;
      if (Math.abs(dx) < Math.abs(dy) * 1.25) { reset(); return; }
      dragging = true;
      node.setPointerCapture(event.pointerId);
    }
    suppressUntil = performance.now() + 400;
  }

  function end(event: PointerEvent) {
    if (!start || event.pointerId !== start.id) return;
    const dx = event.clientX - start.x;
    const threshold = Math.min(72, Math.max(40, node.clientWidth * 0.16));
    const shouldShift = dragging && options.enabled() && Math.abs(dx) >= threshold;
    if (dragging) suppressUntil = performance.now() + 400;
    reset();
    if (shouldShift) options.shift(dx < 0 ? 1 : -1);
  }

  function cancel(event: PointerEvent) {
    if (event.pointerId === start?.id) reset();
  }

  function lostCapture(event: PointerEvent) {
    if (event.target === node) cancel(event);
  }

  function click(event: MouseEvent) {
    if (event.detail > 0 && performance.now() < suppressUntil) {
      event.preventDefault();
      event.stopPropagation();
    }
  }

  node.addEventListener('pointerdown', begin);
  node.addEventListener('pointermove', move);
  node.addEventListener('pointerup', end);
  node.addEventListener('pointercancel', cancel);
  node.addEventListener('lostpointercapture', lostCapture);
  node.addEventListener('click', click, true);

  return {
    update(next: Options) { options = next; },
    destroy() {
      reset();
      node.removeEventListener('pointerdown', begin);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', end);
      node.removeEventListener('pointercancel', cancel);
      node.removeEventListener('lostpointercapture', lostCapture);
      node.removeEventListener('click', click, true);
    },
  };
}
