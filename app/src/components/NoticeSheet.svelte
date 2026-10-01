<script lang="ts">
  import { api } from '../lib/api';
  import { dateTime } from '../lib/format';
  import { cleanHtml } from '../lib/html';
  import { handleAuthError } from '../lib/store.svelte';
  import type { BoardArticle, ClassNotification, ModuleContents } from '../lib/types';
  import FileList from './FileList.svelte';
  import Icon from './Icon.svelte';
  import Sheet from './Sheet.svelte';
  import Skeleton from './Skeleton.svelte';

  let { open = $bindable(false), notice }: { open: boolean; notice: ClassNotification | null } = $props();

  let article = $state<BoardArticle | null>(null);
  let module = $state<ModuleContents | null>(null);
  let error = $state('');

  const board = $derived(notice ? /\/mod\/ubboard\/article\.php\?id=(\d+)&(?:amp;)?bwid=(\d+)/.exec(notice.url) : null);
  const cmid = $derived(Number(board?.[1] ?? /[?&]id=(\d+)/.exec(notice?.url ?? '')?.[1] ?? 0));
  const bwid = $derived(Number(board?.[2] ?? 0));

  $effect(() => {
    if (!open || !notice || !cmid) return;
    article = null;
    module = null;
    error = '';
    const job = board ? api.article(cmid, bwid).then((a) => (article = a)) : api.module(cmid).then((m) => (module = m));
    job.catch((e) => {
      if (!handleAuthError(e)) error = e instanceof Error ? e.message : '내용을 불러오지 못했어요';
    });
  });

  const host = (url: string) => {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  };
</script>

<Sheet bind:open title={board ? (article?.board || '게시판') : module?.modname === 'url' ? '링크' : '자료'}>
  {#if notice}
    <div class="head">
      <span class="course">{notice.course}{notice.section ? ` · ${notice.section}` : ''}</span>
      {#if article}
        <h3>{article.title}</h3>
        <p class="meta">
          {[article.writer, article.posted ? dateTime(article.posted) : ''].filter(Boolean).join(' · ')}
        </p>
      {:else if module}
        <h3>{module.name}</h3>
      {/if}
    </div>

    {#if error}
      <div class="error-box"><Icon name="alert" size={18} />{error}</div>
    {:else if !article && !module}
      <Skeleton rows={1} height={28} />
      <div style="height: 10px"></div>
      <Skeleton rows={3} height={64} />
    {:else if article}
      {#if article.attachments.length}
        <h4 class="sub">첨부파일 {article.attachments.length}개</h4>
        <FileList files={article.attachments} source={(index) => ({ kind: 'board', cmid, bwid, index })} course={notice.course} />
      {/if}
      <div class="body">
        {#if article.html.replace(/<[^>]*>/g, '').trim() || /<img/i.test(article.html)}
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html cleanHtml(article.html)}
        {:else if !article.attachments.length}
          <p class="muted">내용이 없는 글이에요.</p>
        {/if}
      </div>
    {:else if module}
      {#if module.link}
        <a class="link-card" href={module.link} target="_blank" rel="noopener noreferrer">
          <span class="lico"><Icon name="link" size={20} /></span>
          <span class="ltxt"><strong>링크 열기</strong><span>{host(module.link)}</span></span>
          <Icon name="external" size={17} />
        </a>
      {/if}
      {#if module.files.length}
        <h4 class="sub">파일 {module.files.length}개</h4>
        <FileList files={module.files} source={(index) => ({ kind: 'module', cmid, index })} course={notice.course} />
      {:else if !module.link}
        <p class="muted">이 활동에는 받을 파일이 없어요.</p>
      {/if}
    {/if}
  {/if}

  {#snippet footer()}
    {#if notice}
      <a class="btn btn-ghost btn-block" href={notice.url} target="_blank" rel="noopener noreferrer">
        클래스룸에서 열기 <Icon name="external" size={17} />
      </a>
    {/if}
  {/snippet}
</Sheet>

<style>
  .head {
    display: grid;
    gap: 6px;
    padding: 2px 0 14px;
  }

  .course {
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  h3 {
    font-size: 20px;
    font-weight: 750;
    letter-spacing: -0.02em;
    line-height: 1.35;
  }

  .meta {
    font-size: 13px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .sub {
    margin: 4px 2px 8px;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-2);
  }

  .body {
    margin-top: 16px;
    font-size: 14.5px;
    line-height: 1.7;
    overflow-wrap: anywhere;
  }

  .body :global(img) {
    max-width: 100%;
    height: auto;
    border-radius: 10px;
  }

  .body :global(p) {
    margin: 0 0 8px;
  }

  .body :global(table) {
    display: block;
    overflow-x: auto;
    border-collapse: collapse;
  }

  .body :global(td),
  .body :global(th) {
    border: 1px solid var(--border);
    padding: 4px 8px;
  }

  .link-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px;
    margin-bottom: 12px;
    border-radius: 14px;
    border: 1px solid color-mix(in srgb, var(--primary) 35%, var(--border));
    background: color-mix(in srgb, var(--primary) 6%, var(--surface));
    color: var(--text);
    text-decoration: none;
  }

  .lico {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: 12px;
    background: var(--primary-weak);
    color: var(--primary-text);
  }

  .ltxt {
    flex: 1;
    min-width: 0;
    display: grid;
  }

  .ltxt span {
    font-size: 12.5px;
    color: var(--text-3);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
</style>
