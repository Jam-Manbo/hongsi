type Options = {
  canStart: (target: HTMLElement) => boolean;
  move: (distance: number) => void;
  end: (distance: number, velocity: number) => void;
  cancel: () => void;
};
export function verticalDrag(node: HTMLElement, options: Options) {
  let start: { x: number; y: number } | null = null;
  let dragging = false, distance = 0, lastY = 0, lastAt = 0, velocity = 0, suppressUntil = 0;
  let frame = 0;
  const reset = () => {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    node.removeEventListener('touchmove', move);
    start = null; dragging = false; distance = 0; velocity = 0;
  };
  function cancel() { if (dragging) options.cancel(); reset(); }
  function begin(event: TouchEvent) {
    cancel();
    const target = event.target instanceof HTMLElement ? event.target : event.target instanceof Element ? event.target.parentElement : null;
    if (event.touches.length !== 1 || !target || !options.canStart(target)) return;
    const touch = event.touches[0];
    start = { x: touch.clientX, y: touch.clientY };
    lastY = touch.clientY; lastAt = performance.now();
    node.addEventListener('touchmove', move, { passive: false });
  }
  function move(event: TouchEvent) {
    if (!start) return;
    if (event.touches.length !== 1) { cancel(); return; }
    const touch = event.touches[0], dy = touch.clientY - start.y, dx = touch.clientX - start.x;
    if (!dragging) {
      if (Math.abs(dx) < 8 && Math.abs(dy) < 8) return;
      if (dy <= 0 || Math.abs(dx) >= dy) { cancel(); return; }
      dragging = true;
    }
    if (!event.cancelable) { cancel(); return; }
    event.preventDefault();
    const now = performance.now();
    velocity = (touch.clientY - lastY) / Math.max(1, now - lastAt);
    lastY = touch.clientY; lastAt = now;
    distance = Math.max(0, dy - 8);
    if (!frame) frame = requestAnimationFrame(() => { frame = 0; options.move(distance); });
  }
  function end() {
    if (dragging) {
      suppressUntil = performance.now() + 350;
      options.end(distance, performance.now() - lastAt > 100 ? 0 : velocity);
    }
    reset();
  }
  function click(event: MouseEvent) {
    if (performance.now() < suppressUntil) { event.preventDefault(); event.stopPropagation(); }
  }
  node.addEventListener('touchstart', begin, { passive: true });
  node.addEventListener('touchend', end);
  node.addEventListener('touchcancel', cancel);
  node.addEventListener('click', click, true);
  return {
    update(next: Options) { options = next; },
    destroy() {
      reset();
      node.removeEventListener('touchstart', begin);
      node.removeEventListener('touchmove', move);
      node.removeEventListener('touchend', end);
      node.removeEventListener('touchcancel', cancel);
      node.removeEventListener('click', click, true);
    },
  };
}
