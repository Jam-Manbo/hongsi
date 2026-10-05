type ClassroomDestination =
  | { kind: 'item'; key: string }
  | { kind: 'board'; cmid: number; bwid: number }
  | { kind: 'module'; cmid: number };

export function classroomDestination(value: string): ClassroomDestination | null {
  try {
    const url = new URL(value.replace(/&amp;/g, '&'));
    if (url.origin !== 'https://cn2.hongik.ac.kr' || url.username || url.password) return null;
    const id = url.searchParams.get('id') ?? '';
    if (!/^\d+$/.test(id)) return null;
    const cmid = Number(id);
    if (!Number.isSafeInteger(cmid) || cmid <= 0) return null;
    const item = /^\/mod\/(assign|vod)\/view\.php$/.exec(url.pathname);
    if (item) return { kind: 'item', key: `${item[1]}:${cmid}` };
    if (url.pathname === '/mod/ubboard/article.php') {
      const article = url.searchParams.get('bwid') ?? '', bwid = Number(article);
      if (/^\d+$/.test(article) && Number.isSafeInteger(bwid) && bwid > 0) return { kind: 'board', cmid, bwid };
    }
    if (/^\/mod\/(ubfile|resource|folder|url)\/view\.php$/.test(url.pathname)) return { kind: 'module', cmid };
  } catch { /* Unsupported links remain available in the notification list. */ }
  return null;
}
