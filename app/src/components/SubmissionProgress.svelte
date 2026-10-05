<script lang="ts">
  import { isApp } from '../lib/env';
  import type { SubmissionJob, SubmissionStage } from '../lib/types';
  import Icon from './Icon.svelte';

  let { job, delayed, error = '', title, course = '' }: { job: SubmissionJob; delayed: boolean; error?: string; title: string; course?: string } = $props();
  const labels: Record<SubmissionStage, string> = { prepare: '파일 준비', transfer: '홍시로 파일 전송', upload: '클래스룸에 파일 업로드', submit: '제출 처리', verify: '제출 결과 확인' };
  const steps = (isApp ? ['prepare', 'upload', 'submit', 'verify'] : ['prepare', 'transfer', 'upload', 'submit', 'verify']) as SubmissionStage[];
  const current = $derived(steps.indexOf(job.progress.stage));
  const complete = $derived(job.status === 'complete' && !!job.result);
  const percent = $derived(job.progress.totalBytes > 0 ? Math.min(100, Math.floor(job.progress.sentBytes / job.progress.totalBytes * 100)) : null);
  const size = (bytes: number) => bytes >= 1024 * 1024 ? `${(bytes / (1024 * 1024)).toFixed(1)} MB`
    : bytes >= 1024 ? `${Math.round(bytes / 1024)} KB` : `${Math.max(0, bytes)} B`;
  const details = $derived({
    prepare: '파일의 크기와 형식을 확인하고 있어요.',
    transfer: '첨부 파일을 홍시로 보내고 있어요.',
    upload: job.progress.fileCount ? `파일 ${job.progress.uploadedFiles} / ${job.progress.fileCount}개 업로드 완료` : '클래스룸에 연결해 제출 파일을 확인하고 있어요.',
    submit: '클래스룸에서 제출을 처리하고 있어요.',
    verify: '제출 상태와 파일 목록을 다시 확인하고 있어요.',
  });
</script>

<div class="assignment">
  {#if course}<span>{course}</span>{/if}
  <strong>{title}</strong>
</div>
{#if complete && job.result}
  <div class="result" role="status">
    <div class="result-icon"><Icon name="tick" size={25} /></div>
    <h3>과제를 제출했어요.</h3>
    <p>클래스룸에서 제출 완료와<br />파일 {job.result.info.files.length}개를 확인했어요.</p>
  </div>
  <div class="files">
    {#each job.result.info.files as file, i (file.name + i)}
      <div class="file">
        <Icon name="file" size={18} />
        <div><strong>{file.name}</strong>{#if file.size !== null}<span>{size(file.size)}</span>{/if}</div>
        <span class="checked"><Icon name="tick" size={16} /></span>
      </div>
    {/each}
  </div>
{:else}
  <ol aria-label="제출 진행 단계">
    {#each steps as stage, index (stage)}
      {@const active = index === current}
      <li class:done={index < current} class:active class:waiting={active && delayed} class:pending={index > current} aria-current={active ? 'step' : undefined}>
        <span class="dot">
          {#if index < current}<Icon name="tick" size={14} />{:else if active && delayed}<Icon name="alert" size={14} />{:else}{index + 1}{/if}
        </span>
        <div class="step-body">
          <div class="step-title"><span>{labels[stage]}</span>{#if active}<span class="tag">{delayed ? '확인 필요' : '진행 중'}</span>{/if}</div>
          {#if active}
            <p class="detail" role="status">{delayed ? '제출 결과를 아직 확인하지 못했어요.' : details[stage]}</p>
            {#if (stage === 'transfer' || stage === 'upload') && job.progress.totalBytes > 0}
              <div class="upload">
                <div class="file-name">{job.progress.fileName ?? `첨부 파일 ${job.progress.fileCount}개`}</div>
                <div class="meter" role="progressbar" aria-label={stage === 'transfer' ? '홍시로 파일 전송' : '현재 파일 업로드'} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent ?? undefined}>
                  <div style:width="{percent ?? 0}%"></div>
                </div>
                <div class="transfer"><span>{size(job.progress.sentBytes)} / {size(job.progress.totalBytes)}</span><span>{percent}%</span></div>
              </div>
            {/if}
          {/if}
        </div>
      </li>
    {/each}
  </ol>
  {#if delayed}
    <div class="notice" role="status"><strong>제출되었을 수 있어요.</strong><p>다시 제출하기 전에 제출 상태를 확인해 주세요.</p>{#if error}<p>{error}</p>{/if}</div>
  {/if}
{/if}

<style>
  .assignment { display: grid; gap: 3px; padding-bottom: 18px; margin-bottom: 20px; border-bottom: 1px solid var(--border); }
  .assignment > span { color: var(--text-3); font-size: 12px; }
  .assignment > strong { font-size: 14px; font-weight: 650; overflow-wrap: anywhere; }
  ol { list-style: none; padding: 0; margin: 0; display: grid; }
  li { display: grid; grid-template-columns: 24px minmax(0, 1fr); column-gap: 12px; position: relative; padding-bottom: 19px; }
  li:last-child { padding-bottom: 0; }
  li::before { content: ''; width: 1px; background: var(--border); position: absolute; top: 27px; bottom: 3px; left: 11px; }
  li:last-child::before { display: none; }
  .dot { width: 24px; height: 24px; border-radius: 50%; background: var(--surface-2); color: var(--text-3); display: grid; place-items: center; font-size: 11px; font-weight: 650; }
  .done .dot { color: var(--ok); background: var(--ok-weak); }
  .active .dot { color: var(--on-primary); background: var(--primary); }
  .waiting .dot { color: var(--warn); background: var(--warn-weak); }
  .step-title { min-height: 24px; display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 13px; font-weight: 650; }
  .pending .step-title { color: var(--text-3); font-weight: 500; }
  .tag { color: var(--primary-text); font-size: 11px; font-weight: 600; white-space: nowrap; }
  .waiting .tag { color: var(--warn); }
  .detail { font-size: 12px; color: var(--text-3); margin: 4px 0 0; word-break: keep-all; overflow-wrap: anywhere; }
  .upload { padding: 11px 12px; background: var(--surface-2); border-radius: 12px; margin-top: 9px; }
  .file-name { font-size: 12px; font-weight: 600; overflow-wrap: anywhere; }
  .meter { height: 5px; background: var(--border); border-radius: 5px; overflow: hidden; margin: 9px 0 6px; }
  .meter > div { height: 100%; background: var(--primary); border-radius: inherit; transition: width 180ms ease; }
  .transfer { display: flex; justify-content: space-between; gap: 8px; font-size: 11px; color: var(--text-3); font-variant-numeric: tabular-nums; }
  .notice { padding: 12px 13px; margin-top: 18px; border-radius: 12px; background: var(--warn-weak); color: var(--warn); font-size: 13px; word-break: keep-all; overflow-wrap: anywhere; }
  .notice strong { display: block; margin-bottom: 4px; font-weight: 650; }
  .notice p { font-size: 12px; margin: 4px 0 0; }
  .result { text-align: center; padding: 4px 0 2px; }
  .result-icon { width: 48px; height: 48px; display: grid; place-items: center; border-radius: 50%; background: var(--ok-weak); color: var(--ok); margin: 0 auto 14px; }
  .result h3 { font-size: 17px; font-weight: 700; letter-spacing: -.025em; margin: 0; }
  .result p { margin: 6px 0 0; font-size: 13px; color: var(--text-3); }
  .files { display: grid; gap: 8px; margin-top: 18px; }
  .file { display: flex; gap: 10px; align-items: center; background: var(--surface-2); border-radius: 12px; padding: 12px; color: var(--text-3); }
  .file > div { flex: 1; min-width: 0; }
  .file strong { display: block; color: var(--text); font-size: 13px; font-weight: 600; overflow-wrap: anywhere; }
  .file div > span { display: block; font-size: 12px; font-variant-numeric: tabular-nums; }
  .checked { color: var(--ok); display: flex; }
  @media(max-width: 420px) { li { column-gap: 10px; } }
  @media(prefers-reduced-motion: reduce) { .meter > div { transition: none; } }
</style>
