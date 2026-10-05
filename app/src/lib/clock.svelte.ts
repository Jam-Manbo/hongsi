export const clock = $state({ now: Date.now() });

export function watchClock(): () => void {
  const refresh = () => { clock.now = Date.now(); };
  const resume = () => { if (document.visibilityState === 'visible') refresh(); };
  refresh();
  const timer = window.setInterval(resume, 1000);
  window.addEventListener('focus', resume);
  window.addEventListener('pageshow', resume);
  document.addEventListener('visibilitychange', resume);
  return () => {
    window.clearInterval(timer);
    window.removeEventListener('focus', resume);
    window.removeEventListener('pageshow', resume);
    document.removeEventListener('visibilitychange', resume);
  };
}
