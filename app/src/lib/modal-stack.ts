type Panel = { node: HTMLElement; backdrop: HTMLElement; close: () => boolean; previous: HTMLElement | null };
type Popup = { node: HTMLElement; anchor: HTMLElement; close: () => void };
const panels: Panel[] = [];
const popups: Popup[] = [];
const savedInert = new Map<HTMLElement, boolean>();
const HISTORY_KEY = 'hongsiSheet';
let historyToken = '';
let historySequence = 0;
let historyUrl = '';
let removingHistory = false;
let observer: MutationObserver | undefined;
let bodyOverflow = '';

const top = () => panels.at(-1);
export const isTopSheet = (node: HTMLElement | undefined) => top()?.node === node;

function activePopups() {
  const panel = top();
  return popups.filter((popup) => panel?.node.contains(popup.anchor));
}

function focusables() {
  const panel = top();
  if (!panel) return [];
  return [panel.node, ...activePopups().map((popup) => popup.node)]
    .flatMap((root) => [...root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea, [tabindex], [contenteditable="true"]')])
    .filter((node) => node.tabIndex >= 0 && !node.matches(':disabled') && !node.closest('[inert]') && node.getClientRects().length > 0);
}

function containsFocus(node: Node | null) {
  return !!node && (top()?.node.contains(node) || activePopups().some((popup) => popup.node.contains(node)));
}

function onFocus(event: FocusEvent) {
  if (!containsFocus(event.target as Node)) top()?.node.focus({ preventScroll: true });
}

function onKey(event: KeyboardEvent) {
  if (event.defaultPrevented || !top()) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    top()!.close();
  } else if (event.key === 'Tab') {
    const nodes = focusables();
    const index = nodes.indexOf(document.activeElement as HTMLElement);
    if (index < 0 || (!event.shiftKey && index === nodes.length - 1) || (event.shiftKey && index === 0)) {
      event.preventDefault();
      (event.shiftKey ? nodes.at(-1) : nodes[0])?.focus({ preventScroll: true });
      if (!nodes.length) top()!.node.focus({ preventScroll: true });
    }
  }
}

function pushHistory() {
  historyUrl = location.href;
  history.pushState({ ...history.state, [HISTORY_KEY]: historyToken }, '', historyUrl);
}

function onBack() {
  if (removingHistory) {
    removingHistory = false;
    if (top()) pushHistory();
    return;
  }
  if (!top() || history.state?.[HISTORY_KEY] === historyToken) return;
  const popup = activePopups().at(-1);
  if (popup) {
    popup.close();
    popup.anchor.focus({ preventScroll: true });
    pushHistory();
    return;
  }
  const more = panels.length > 1;
  // A rejected close may open a discard confirmation. Keep Back available for it.
  if (!top()!.close() || more) pushHistory();
}

if (typeof window !== 'undefined') window.addEventListener('popstate', onBack);

function sync() {
  const active = top();
  for (const [index, panel] of panels.entries()) {
    const covered = panel !== active;
    panel.node.style.zIndex = String(61 + index * 3);
    panel.backdrop.style.zIndex = String(60 + index * 3);
    panel.node.toggleAttribute('data-covered', covered);
    panel.node.toggleAttribute('data-nested', index > 0);
    panel.backdrop.toggleAttribute('data-covered', covered);
    panel.node.setAttribute('aria-modal', String(!covered));
    if (covered) panel.node.setAttribute('aria-hidden', 'true');
    else panel.node.removeAttribute('aria-hidden');
  }
  for (const popup of [...popups]) {
    if (!popup.anchor.isConnected || (active && !active.node.contains(popup.anchor))) {
      popup.node.inert = true;
      popup.close();
    } else {
      popup.node.style.zIndex = String(active ? 62 + (panels.length - 1) * 3 : 75);
    }
  }
  if (!active) {
    for (const [node, inert] of savedInert) node.inert = inert;
    savedInert.clear();
    return;
  }
  const allowed = new Set<HTMLElement>([active.node, active.backdrop, ...activePopups().map((popup) => popup.node)]);
  for (const node of document.body.children) {
    if (!(node instanceof HTMLElement) || node.matches('script, style, link')) continue;
    if (!savedInert.has(node)) savedInert.set(node, node.inert);
    node.inert = allowed.has(node) ? false : true;
  }
}

export function registerSheet(node: HTMLElement, backdrop: HTMLElement, close: () => boolean) {
  const entry = { node, backdrop, close, previous: document.activeElement as HTMLElement | null };
  if (!panels.length) {
    bodyOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    document.addEventListener('focusin', onFocus);
    window.addEventListener('keydown', onKey);
    observer = new MutationObserver(sync);
    observer.observe(document.body, { childList: true });
    historyToken = `${Date.now()}:${++historySequence}`;
    if (!removingHistory) pushHistory();
  }
  panels.push(entry);
  // Move focus before hiding the previous dialog from assistive technology.
  node.inert = false;
  node.focus({ preventScroll: true });
  sync();
  return () => {
    const wasTop = top() === entry;
    panels.splice(panels.indexOf(entry), 1);
    if (!panels.length) {
      observer?.disconnect();
      observer = undefined;
      document.body.style.overflow = bodyOverflow;
      document.removeEventListener('focusin', onFocus);
      window.removeEventListener('keydown', onKey);
      if (history.state?.[HISTORY_KEY] === historyToken && location.href === historyUrl) {
        removingHistory = true;
        history.back();
      }
    }
    sync();
    // An outgoing transition may leave the old DOM visible for a moment.
    node.inert = true;
    node.setAttribute('aria-hidden', 'true');
    node.setAttribute('aria-modal', 'false');
    backdrop.inert = true;
    if (top()) backdrop.setAttribute('data-covered', '');
    if (wasTop) {
      const previous = entry.previous;
      if (previous?.isConnected && !previous.closest('[inert]') && (!top() || containsFocus(previous))) previous.focus({ preventScroll: true });
      else top()?.node.focus({ preventScroll: true });
    }
  };
}

export function registerSheetPopup(node: HTMLElement, anchor: HTMLElement, close: () => void) {
  const popup = { node, anchor, close };
  popups.push(popup);
  sync();
  return () => {
    popups.splice(popups.indexOf(popup), 1);
    sync();
  };
}

export function closeSheets() {
  for (const panel of [...panels].reverse()) if (!panel.close()) return false;
  return true;
}
