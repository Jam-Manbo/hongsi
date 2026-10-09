<script lang="ts">
  import { onDestroy, untrack } from 'svelte';
  import { api } from '../../shared/api/api';
  import { dueDateTime, sentenceLines } from '../../shared/utils/format';
  import { errorText, writeBlocked } from '../../shared/api/net.svelte';
  import { handleAuthError } from '../auth/auth-state.svelte';
  import { isCurrentSession, sessionVersion } from '../../shared/state/session';
  import { clearSubmission, resumeSubmission, sendSubmission, submissionOperation } from './submission.svelte';
  import type { CalendarItem, SubmissionView } from '../../shared/types';
  import Icon from '../../shared/ui/Icon.svelte';
  import Sheet from '../../shared/ui/Sheet.svelte';
  import Skeleton from '../../shared/ui/Skeleton.svelte';
  import SubmissionProgress from './SubmissionProgress.svelte';

  let { open = $bindable(false), item, course = '' }: { open: boolean; item: CalendarItem | null; course?: string } = $props();

  let view = $state<SubmissionView | null>(null);
  let error = $state('');
  let keep = $state(new Set<string>());
  let added = $state<File[]>([]);
  let statement = $state(false);
  let lateChecked = $state(false);
  let step = $state<'edit' | 'confirm'>('edit');
  let picker: HTMLInputElement | undefined = $state();
  let generation = 0;
  onDestroy(() => { generation += 1; });

  const cmid = $derived(item ? Number(item.key.split(':')[1]) : 0);
  const operation = $derived(submissionOperation(cmid));
  const showProgress = $derived(!!operation.job && operation.job.status !== 'failed');
  const complete = $derived(operation.job?.status === 'complete');
  const busy = $derived(operation.checking || (showProgress && !complete && !operation.delayed));
  const failure = $derived(error || (operation.job?.status === 'failed' ? operation.error : ''));
  const progressLabel = $derived(operation.job ? ({ prepare: '파일 준비 중…', transfer: '홍시로 파일 전송 중…', upload: '클래스룸에 업로드 중…', submit: '제출 처리 중…', verify: '제출 결과 확인 중…' })[operation.job.progress.stage] : '제출 준비 중…');

  $effect(() => {
    const request = ++generation;
    if (!open || !cmid) return;
    const version = sessionVersion();
    view = null;
    error = '';
    added = [];
    statement = false;
    lateChecked = false;
    step = 'edit';
    untrack(() => { void resumeSubmission(operation); });
    api
      .submission(cmid)
      .then((v) => {
        if (request !== generation || !isCurrentSession(version)) return;
        view = v;
        keep = new Set(v.info.files.map((f) => f.name));
      })
      .catch((e) => {
        if (request !== generation || !isCurrentSession(version)) return;
        if (!handleAuthError(e)) error = errorText(e, '제출 정보를 읽지 못했어요.');
      });
    return () => { generation += 1; };
  });

  const total = $derived(keep.size + added.length);
  const max = $derived(view?.config.maxFiles ?? 1);
  const SEND_LIMIT = 100 * 1024 * 1024;
  const perFile = $derived(Math.min(view && view.config.maxBytes > 0 ? view.config.maxBytes : Infinity, SEND_LIMIT));
  const addedBytes = $derived(added.reduce((n, f) => n + f.size, 0));
  const tooBig = $derived(added.filter((f) => f.size > perFile));
  const edited = $derived(view?.info.status === 'submitted');
  const canSend = $derived(
    !!view &&
      !view.closed &&
      total > 0 &&
      total <= max &&
      tooBig.length === 0 &&
      addedBytes <= SEND_LIMIT &&
      (!view.config.statement || statement),
  );

  const MB = 1024 * 1024;
  const size = (n: number | null) => {
    if (n === null) return '';
    if (n >= 1024 * MB) return `${+(n / 1024 / MB).toFixed(1)}GB`;
    if (n >= MB) return `${+(n / MB).toFixed(1)}MB`;
    return `${Math.max(1, Math.round(n / 1024))}KB`;
  };

  function pick(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    added = [...added, ...Array.from(input.files ?? [])];
    input.value = '';
  }

  function toggleKeep(name: string) {
    const next = new Set(keep);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    keep = next;
  }

  async function send() {
    if (busy || !view || !item) return;
    if (writeBlocked('school', '제출할')) return;
    error = '';
    step = 'edit';
    await sendSubmission(operation, [...keep], added, view.late && lateChecked, statement);
  }

  function close() {
    if (complete) clearSubmission(operation);
    open = false;
  }

  async function inspectFiles() {
    if (operation.checking) return;
    const request = generation, version = sessionVersion(), target = operation;
    target.checking = true;
    try {
      const current = await api.submission(cmid);
      if (request !== generation || !isCurrentSession(version)) return;
      view = current;
      keep = new Set(current.info.files.map((file) => file.name));
      added = [];
      step = 'edit';
      clearSubmission(target);
    } catch (e) {
      if (request === generation && isCurrentSession(version)) target.error = errorText(e, '제출 파일을 확인하지 못했어요.');
    } finally { target.checking = false; }
  }
</script>

<Sheet bind:open onbeforeclose={() => !busy} onclose={close} closeDisabled={busy} title={showProgress ? (complete ? '제출 완료' : operation.delayed ? '제출 결과 확인' : '과제를 제출하고 있어요') : step === 'confirm' ? (view?.late ? '마감이 지난 과제예요.' : '제출할까요?') : edited ? '제출 파일 수정' : '과제 제출'}>
  {#if showProgress && operation.job}
    <SubmissionProgress job={operation.job} delayed={operation.delayed} error={operation.error} title={item?.title ?? '과제'} {course} />
  {:else}
  {#if failure}<div class="error-box"><Icon name="alert" size={18} /><span class="sentence-message">{sentenceLines(failure)}</span></div>{/if}
  {#if !view && !failure}
    <Skeleton rows={3} height={52} />
  {:else if view && step === 'edit'}
    <div class="due" class:late={view.late}>
      <Icon name={view.late ? 'alert' : 'clock'} size={17} />
      {#if view.due}마감 {dueDateTime(view.due)}{:else}마감 없음{/if}
      {#if view.late}<span class="chip danger">마감 지남 · 지각 제출</span>{/if}
    </div>
    {#if view.closed}
      <p class="closed">
        {view.cutoff && view.cutoff * 1000 < Date.now() ? '제출 기한이 지나 더 이상 제출할 수 없어요.' : '지금은 파일을 제출하거나 수정할 수 없어요.'}
      </p>
    {:else}
      {#if view.info.files.length}
        <h4><span class="file-heading">{edited ? '지금 제출된 파일' : '첨부된 파일'}</span> <span class="muted">빼려면 누르세요.</span></h4>
        <div class="list">
          {#each view.info.files as f (f.name)}
            <button class="file" class:off={!keep.has(f.name)} onclick={() => toggleKeep(f.name)} aria-pressed={keep.has(f.name)}>
              <Icon name={keep.has(f.name) ? 'tick' : 'close'} size={16} stroke={2.4} />
              <span class="name">{f.name}</span>
              <span class="muted">{keep.has(f.name) ? size(f.size) : '뺄 예정'}</span>
            </button>
          {/each}
        </div>
      {/if}

      <h4>
        <span class="file-heading">새로 올릴 파일</span>
        <span class="muted">
          {total}/{max}개 · 파일당 {size(perFile)}까지{view.config.maxBytes > SEND_LIMIT ? ` (클래스룸 설정 ${size(view.config.maxBytes)})` : ''}
        </span>
      </h4>
      {#if total > max}<p class="warn-line">파일은 최대 {max}개까지 제출할 수 있어요.</p>{/if}
      {#if addedBytes > SEND_LIMIT}<p class="warn-line">큰 파일은 클래스룸에서 직접 올려 주세요.</p>{/if}
      <div class="list">
        {#each added as f, i (f.name + i)}
          <div class="file new" class:bad={tooBig.includes(f)}>
            <Icon name="file" size={16} />
            <span class="name">{f.name}</span>
            <span class="muted">{tooBig.includes(f) ? '너무 커요' : size(f.size)}</span>
            <button class="x" onclick={() => (added = added.filter((_, j) => j !== i))} aria-label="{f.name} 빼기"><Icon name="close" size={15} /></button>
          </div>
        {/each}
        <button class="add" onclick={() => picker?.click()} disabled={total >= max}>
          <Icon name="plus" size={17} stroke={2.4} />{total >= max ? `최대 ${max}개예요` : '파일 고르기'}
        </button>
        <input bind:this={picker} type="file" multiple hidden onchange={pick} />
      </div>

      {#if view.config.statement}
        <label class="statement">
          <input type="checkbox" bind:checked={statement} />
          다른 사람의 것을 베끼지 않았음을 확인해요 (제출 서약)
        </label>
      {/if}
      {#if view.config.text && !view.config.files}
        <p class="warn">내용을 직접 입력해 제출하는 과제예요. 클래스룸에서 제출해 주세요.</p>
      {/if}
    {/if}
  {:else if view && step === 'confirm'}
    {#if view.late}
      <div class="late-box">
        <Icon name="alert" size={22} />
        <div>
          <strong>마감({view.due ? dueDateTime(view.due) : ''})이 지났어요.</strong>
          <p>지금 {edited ? '제출한 파일을 수정하면' : '제출하면'} 클래스룸에 <b>지각 제출</b>로 표시돼요.{#if edited}<br />기한 안에 제출했더라도 지각 제출로 바뀔 수 있어요.{/if}</p>
        </div>
      </div>
      <label class="statement strong">
        <input type="checkbox" bind:checked={lateChecked} />
        지각 제출로 기록되는 것을 확인했어요.
      </label>
    {:else}
      <p class="confirm">{edited ? '수정한 파일이 클래스룸에 다시 제출돼요.' : '파일이 클래스룸에 제출돼요.'}<br />파일 {total}개를 보낼게요.</p>
    {/if}
  {/if}
  {/if}

  {#snippet footer()}
    {#if showProgress}
      {#if complete}
        <button class="btn btn-primary w2" onclick={close}>확인</button>
      {:else if operation.delayed}
        <button class="btn btn-ghost w1" disabled={operation.checking} onclick={close}>닫기</button>
        <button class="btn btn-primary w2" disabled={operation.checking} onclick={() => operation.missing ? inspectFiles() : resumeSubmission(operation)}>{operation.checking ? '제출 결과 확인 중…' : operation.missing ? '제출 파일 확인' : '제출 상태 확인'}</button>
      {:else}
        <button class="btn btn-ghost w2" disabled>{operation.checking ? '제출 결과 확인 중…' : progressLabel}</button>
      {/if}
    {:else if step === 'edit'}
      <button class="btn btn-ghost w1" disabled={busy} onclick={close}>닫기</button>
      <button class="btn btn-primary w2" disabled={busy || !canSend} onclick={() => (step = 'confirm')}>
        <Icon name="check" size={18} />{edited ? '제출 파일 수정' : '제출하기'}
      </button>
    {:else}
      <button class="btn btn-ghost w1" onclick={() => (step = 'edit')} disabled={busy}>뒤로</button>
      <button class="btn {view?.late ? 'btn-danger' : 'btn-primary'} w2" disabled={busy || (view?.late && !lateChecked)} onclick={send}>
        {busy ? '보내는 중…' : view?.late ? '지각 제출하기' : '제출하기'}
      </button>
    {/if}
  {/snippet}
</Sheet>

<style>
  .error-box { margin-bottom: 14px; }
  .due {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--surface-2);
    font-size: 14px;
    font-weight: 650;
    margin-bottom: 14px;
  }

  .due.late {
    background: var(--danger-weak);
    color: var(--danger);
  }

  h4 {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 8px;
    justify-content: space-between;
    font-size: 13px;
    font-weight: 700;
    color: var(--text-2);
    margin: 14px 2px 8px;
  }

  h4 .muted {
    font-weight: 550;
  }

  .file-heading {
    flex: none;
  }

  .late-box b {
    white-space: nowrap;
  }

  .list {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
  }

  .file {
    min-width: 0;
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    font-size: 13.5px;
    text-align: left;
  }

  .file .name {
    flex: 1;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .file .muted {
    font-size: 12px;
  }

  .file.off {
    opacity: 0.55;
    text-decoration: line-through;
  }

  .file.new {
    border-style: dashed;
  }

  .file.bad {
    border-color: var(--danger);
    color: var(--danger);
  }

  .x {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 8px;
    color: var(--text-3);
  }

  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    height: 46px;
    border-radius: 12px;
    border: 1.5px dashed var(--border-strong);
    color: var(--primary-text);
    font-weight: 700;
    font-size: 14px;
  }

  .add:disabled {
    color: var(--text-3);
  }

  .warn,
  .closed {
    margin-top: 10px;
    font-size: 13.5px;
    color: var(--warn);
    font-weight: 600;
  }

  .closed {
    color: var(--danger);
  }

  .statement {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin-top: 14px;
    font-size: 13.5px;
    color: var(--text-2);
  }

  .statement input {
    margin-top: 3px;
    width: 18px;
    height: 18px;
    accent-color: var(--primary);
  }

  .statement.strong {
    font-weight: 700;
    color: var(--danger);
  }

  .statement.strong input {
    accent-color: var(--danger);
  }

  .late-box {
    display: flex;
    gap: 12px;
    padding: 14px;
    border-radius: 14px;
    background: var(--danger-weak);
    color: var(--danger);
  }

  .late-box p {
    margin-top: 4px;
    font-size: 14px;
    line-height: 1.6;
    color: var(--text);
  }

  .confirm {
    font-size: 14.5px;
    color: var(--text-2);
    line-height: 1.6;
  }

  .warn-line {
    margin: -2px 0 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--danger);
  }
</style>
