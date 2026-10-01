<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api } from '../lib/api';
  import { downloadFile, downloads, fileId } from '../lib/actions.svelte';
  import { errorText } from '../lib/net.svelte';
  import { calendar, handleAuthError, notices } from '../lib/store.svelte';
  import { readUserData, writeUserData } from '../lib/session';
  import { focus, go, toastOnce } from '../lib/ui.svelte';
  import type { ClassNotification } from '../lib/types';
  import EmptyState from './EmptyState.svelte';
  import Icon from './Icon.svelte';
  import LoadError from './LoadError.svelte';
  import NoticeSheet from './NoticeSheet.svelte';
  import Sheet from './Sheet.svelte';
  import Skeleton from './Skeleton.svelte';

  let open = $state(false);
  let seen = $state(new Set<string>(readUserData<string[]>('notices-seen', [])));
  let viewing = $state<ClassNotification | null>(null);
  let viewOpen = $state(false);
  let pending = $state('');

  const list = $derived(notices.data ?? []);
  const unread = $derived(list.filter((n) => !seen.has(n.url)).length);

  const KIND: Record<string, { icon: string; label: string; tone?: string }> = {
    assign: { icon: 'file', label: '과제', tone: 'warn' },
    vod: { icon: 'play', label: '강의', tone: 'primary' },
    ubfile: { icon: 'download', label: '파일', tone: 'info' },
    resource: { icon: 'download', label: '파일', tone: 'info' },
    folder: { icon: 'folder', label: '폴더', tone: 'info' },
    url: { icon: 'link', label: '링크', tone: 'info' },
    ubboard_notice: { icon: 'megaphone', label: '공지', tone: 'danger' },
    ubboard: { icon: 'chat', label: '게시판' },
    ubboard_qna: { icon: 'chat', label: '질문' },
    forum: { icon: 'chat', label: '토론' },
    quiz: { icon: 'quiz', label: '퀴즈', tone: 'warn' },
    zoom: { icon: 'video', label: '화상강의', tone: 'primary' },
    feedback: { icon: 'list', label: '설문' },
    survey: { icon: 'list', label: '설문' },
    choice: { icon: 'list', label: '투표' },
    page: { icon: 'book', label: '페이지' },
    lesson: { icon: 'book', label: '학습' },
    workshop: { icon: 'book', label: '상호평가' },
    ubpeer: { icon: 'book', label: '동료평가' },
    wiki: { icon: 'book', label: '위키' },
    glossary: { icon: 'book', label: '용어집' },
    chat: { icon: 'chat', label: '채팅' },
    scorm: { icon: 'play', label: '콘텐츠' },
    econtents: { icon: 'play', label: '콘텐츠' },
    course: { icon: 'book', label: '강좌' },
  };
  const kind = (n: ClassNotification) => KIND[n.kind] ?? { icon: 'bell', label: '알림' };

  $effect(() => {
    if (!focus.notices) return;
    focus.notices = false;
    untrack(show);
  });

  function show() {
    open = true;
    notices.load(true);
  }

  function itemKey(url: string): string | null {
    const m = /\/mod\/(assign|vod)\/view\.php\?id=(\d+)/.exec(url);
    return m ? `${m[1]}:${m[2]}` : null;
  }

  function view(n: ClassNotification) {
    viewing = n;
    viewOpen = true;
  }

  async function quickDownload(n: ClassNotification, cmid: number) {
    pending = n.url;
    try {
      const m = await api.module(cmid);
      if (m.files.length === 1) await downloadFile({ kind: 'module', cmid, index: 0 }, m.files[0].name, n.course);
      else view(n);
    } catch (e) {
      if (!handleAuthError(e)) {
        toastOnce(errorText(e, '파일을 받지 못했어요'), 'error');
        view(n);
      }
    } finally {
      pending = '';
    }
  }

  function openNotice(e: MouseEvent, n: ClassNotification) {
    const key = itemKey(n.url);
    if (key) {
      const known = calendar.data ? calendar.data.items.some((i) => i.key === key) : true;
      if (!known) return;
      e.preventDefault();
      open = false;
      focus.item = key;
      go('calendar');
      return;
    }
    if (/\/mod\/ubboard\/article\.php\?id=\d+&(?:amp;)?bwid=\d+/.test(n.url)) {
      e.preventDefault();
      view(n);
      return;
    }
    const mod = /\/mod\/(ubfile|resource|folder|url)\/view\.php\?id=(\d+)/.exec(n.url);
    if (!mod) return;
    e.preventDefault();
    if (pending) return;
    if (mod[1] === 'ubfile' || mod[1] === 'resource') void quickDownload(n, Number(mod[2]));
    else view(n);
  }

  const busyFor = (n: ClassNotification) =>
    pending === n.url || downloads.busy === fileId({ kind: 'module', cmid: Number(/[?&]id=(\d+)/.exec(n.url)?.[1] ?? 0), index: 0 });

  function markSeen() {
    const urls = list.map((n) => n.url);
    seen = new Set([...urls, ...seen].slice(0, 300));
    writeUserData('notices-seen', [...seen]);
    if (urls.length) void api.markNoticesSeen(urls.slice(0, 200)).catch(() => {});
  }

  async function syncSeen() {
    try {
      const { urls } = await api.noticesSeen();
      const merged = new Set([...seen, ...urls]);
      if (merged.size === seen.size) return;
      seen = new Set([...merged].slice(0, 300));
      writeUserData('notices-seen', [...seen]);
    } catch {
    }
  }

  const onVisible = () => {
    if (document.visibilityState === 'visible') void syncSeen();
  };

  onMount(() => {
    notices.load();
    void syncSeen();
    const t = setInterval(() => {
      notices.load();
      void syncSeen();
    }, 5 * 60_000);
    document.addEventListener('visibilitychange', onVisible);
    return () => {
      clearInterval(t);
      document.removeEventListener('visibilitychange', onVisible);
    };
  });
</script>

<button class="bell" onclick={show} aria-label={unread ? `알림 ${unread}개 새로 옴` : '알림'}>
  <Icon name="bell" size={22} />
  {#if unread}<span class="badge">{unread > 9 ? '9+' : unread}</span>{/if}
</button>

<Sheet bind:open title="클래스룸 알림" onclose={markSeen}>
  <LoadError resource={notices} what="알림을" />
  {#if !notices.data}
    {#if !notices.error}<Skeleton rows={4} height={58} />{/if}
  {:else if list.length}
    <ul>
      {#each list as n (n.url + n.when)}
        {@const k = kind(n)}
        {@const busy = busyFor(n)}
        <li class:new={!seen.has(n.url)}>
          <a href={n.url} target="_blank" rel="noopener noreferrer" onclick={(e) => openNotice(e, n)} aria-busy={busy}>
            <span class="ico {k.tone ?? ''}" class:spin={busy}><Icon name={busy ? 'refresh' : k.icon} size={18} /></span>
            <span class="txt">
              <strong>{n.message}</strong>
              <span class="muted"><b>{k.label}</b> · {n.course}{n.section ? ` · ${n.section}` : ''}</span>
            </span>
            <span class="when">{busy ? '받는 중' : n.when.replace(/(\d+)(분|시간|일)전/, '$1$2 전')}</span>
          </a>
        </li>
      {/each}
    </ul>
  {:else}
    <EmptyState message="새 알림이 없어요" detail="새 과제·자료·공지가 올라오면 여기에 떠요" />
  {/if}
</Sheet>

<NoticeSheet bind:open={viewOpen} notice={viewing} />

<style>
  .bell {
    position: relative;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    color: var(--text-2);
  }

  .bell:hover {
    background: var(--surface-3);
  }

  .badge {
    position: absolute;
    top: 3px;
    right: 2px;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--danger);
    color: #fff;
    font-size: 11px;
    font-weight: 800;
    line-height: 18px;
    text-align: center;
    box-shadow: 0 0 0 2px var(--bg);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }

  a {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: 14px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: inherit;
    text-decoration: none;
  }

  a:hover {
    background: var(--surface-2);
  }

  li.new a {
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
    background: color-mix(in srgb, var(--primary) 5%, var(--surface));
  }

  .ico {
    flex: none;
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: 11px;
    background: var(--surface-3);
    color: var(--text-2);
  }

  .ico.warn {
    background: var(--warn-weak);
    color: var(--warn);
  }

  .ico.info {
    background: var(--info-weak);
    color: var(--info);
  }

  .ico.primary {
    background: var(--primary-weak);
    color: var(--primary-text);
  }

  .ico.danger {
    background: var(--danger-weak);
    color: var(--danger);
  }

  .txt b {
    font-weight: 700;
    color: var(--text-2);
  }

  .txt {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  .txt strong {
    font-size: 14px;
    font-weight: 650;
  }

  .txt .muted {
    font-size: 12.5px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .when {
    flex: none;
    font-size: 12px;
    color: var(--text-3);
  }

  li.new .when {
    color: var(--primary-text);
    font-weight: 700;
  }
</style>
