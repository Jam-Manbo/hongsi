export const isApp = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
export const isIOS = isApp && (/iPhone|iPad|iPod/i.test(navigator.userAgent) || (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1));
export const isAndroid = isApp && /Android/i.test(navigator.userAgent);
export const hasWidgets = isIOS || isAndroid;
export const hasStudentCard = isIOS || isAndroid;
