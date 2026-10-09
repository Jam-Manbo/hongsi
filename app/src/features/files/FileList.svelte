<script lang="ts">
  import { isApp } from '../../shared/api/api';
  import { downloadFile, downloads, fileId, openDownload, revealDownload, savedFile } from './download-state.svelte';
  import type { Attachment, FileSource } from '../../shared/types';
  import Icon from '../../shared/ui/Icon.svelte';

  let {
    files,
    source,
    course,
  }: {
    files: Attachment[];
    source: (index: number) => FileSource;
    course: string;
  } = $props();

  const size = (n: number | null) =>
    n === null ? '' : n > 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)}MB` : `${Math.max(1, Math.round(n / 1024))}KB`;

  function press(index: number) {
    const got = savedFile(source(index));
    if (got) void openDownload(got);
    else void downloadFile(source(index), files[index].name, course);
  }
</script>

<ul class="files">
  {#each files as f, i (i)}
    {@const got = savedFile(source(i))}
    {@const busy = downloads.busy === fileId(source(i))}
    <li class:got>
      <button class="fmain" onclick={() => press(i)} disabled={busy} title={got ? '눌러서 바로 열기' : '눌러서 받기'}>
        <span class="ficon" class:spin={busy}><Icon name={busy ? 'refresh' : got ? 'tick' : 'download'} size={17} stroke={got ? 2.4 : 1.9} /></span>
        <span class="fname">{f.name}</span>
        <span class="fsize">{busy ? '받는 중…' : got ? '받음' : size(f.size)}</span>
      </button>
      {#if got && isApp}
        <div class="fbtns">
          <button class="fbtn" onclick={() => openDownload(got)}>열기</button>
          <button class="fbtn" onclick={() => revealDownload(got)}>폴더에서 열기</button>
        </div>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }

  li {
    display: grid;
    gap: 4px;
    padding: 4px;
    border-radius: 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
  }

  .fmain {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border-radius: 9px;
    text-align: left;
  }

  .fmain:hover {
    background: var(--surface-3);
  }

  .ficon {
    display: inline-flex;
    color: var(--primary-text);
  }

  .got .ficon {
    color: var(--ok);
  }

  .fname {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .fsize {
    flex: none;
    font-size: 12px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .got .fsize {
    color: var(--ok);
    font-weight: 700;
  }

  .fbtns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    padding: 0 4px 4px;
  }

  .fbtn {
    height: 34px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 700;
    color: var(--primary-text);
    background: var(--primary-weak);
  }
</style>
