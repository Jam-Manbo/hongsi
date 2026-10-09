<script lang="ts">
  import { onMount } from 'svelte';
  import { sentenceLines } from '../shared/utils/format';
  import Icon from '../shared/ui/Icon.svelte';
  type Release = { version: string; versionCode: number; url: string; size: number; notes: string; sha256: string };
  type ReleaseSummary = { id: string; tag: string; version: string; publishedAt: string | null; url: string; size: number; notesUrl: string; prerelease: boolean; isLatest: boolean };
  const versionsPage = location.pathname.replace(/\/$/, '') === '/download/versions';
  const iosPage = location.pathname.replace(/\/$/, '') === '/download/ios';
  const iosSteps = [
    { title: 'Safari에서 공유 메뉴 열기', text: 'Safari에서 홍시에 접속한 뒤 공유 버튼을 눌러 주세요. 공유 버튼이 보이지 않으면 주소창 옆 메뉴에서 찾을 수 있어요.', image: 'safari-share.png', alt: 'Safari의 홍시 화면에서 공유 메뉴를 여는 모습' },
    { title: '홈 화면에 추가 선택하기', text: '공유 메뉴를 아래로 내려 홈 화면에 추가를 눌러 주세요.', image: 'add-to-home.png', alt: 'Safari 공유 메뉴의 홈 화면에 추가 항목' },
    { title: '이름을 확인하고 추가하기', text: '이름을 홍시로 정하고 추가를 눌러 주세요. 웹 앱으로 열기 옵션이 보이면 켜 주세요.', image: 'confirm-add.png', alt: '홍시 이름과 웹 앱으로 열기 옵션을 확인한 뒤 추가하는 화면' },
  ];
  let history = $state<ReleaseSummary[]>([]);
  let historyUrl = $state('');
  let historyLoading = $state(true);
  let historyError = $state('');
  let releaseTab = $state<'stable' | 'beta'>('stable');
  let visibleCount = $state(5);
  const visibleHistory = $derived(history.filter(item => item.prerelease === (releaseTab === 'beta')));
  function selectReleaseTab(tab: 'stable' | 'beta') {
    releaseTab = tab;
    visibleCount = 5;
    expandedId = null;
  }
  function moveReleaseTab(event: KeyboardEvent) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    selectReleaseTab(event.key === 'Home' ? 'stable' : event.key === 'End' ? 'beta' : releaseTab === 'stable' ? 'beta' : 'stable');
    document.getElementById(`release-tab-${releaseTab}`)?.focus();
  }
  let expandedId = $state<string | null>(null);
  let notes = $state<Record<string, { loading: boolean; error: string; text: string | null }>>({});
  async function loadNotes(item: ReleaseSummary) {
    const cached = notes[item.id];
    if (cached?.loading || cached?.text != null) return;
    notes[item.id] = { loading: true, error: '', text: null };
    try {
      const response = await fetch(`/api/app-releases/${encodeURIComponent(item.id)}`, { cache: 'no-store', credentials: 'omit' });
      if (!response.ok) throw new Error();
      const data = await response.json();
      if (data.version !== item.version || data.url !== item.url || data.size !== item.size || typeof data.notes !== 'string') throw new Error();
      notes[item.id] = { loading: false, error: '', text: data.notes };
    } catch { notes[item.id] = { loading: false, error: '변경사항을 불러오지 못했어요.', text: null }; }
  }
  function toggleVersion(item: ReleaseSummary) {
    expandedId = expandedId === item.id ? null : item.id;
    if (expandedId) void loadNotes(item);
  }
  let release = $state<Release | null>(null);
  function secureLink(value: unknown): value is string {
    if (typeof value !== 'string') return false;
    try { const url = new URL(value); return url.protocol === 'https:' && !url.username && !url.password; }
    catch { return false; }
  }
  function publishedDate(value: string | null) {
    if (!value || !Number.isFinite(Date.parse(value))) return '';
    return new Intl.DateTimeFormat('ko-KR', { year: 'numeric', month: 'long', day: 'numeric', timeZone: 'Asia/Seoul' }).format(new Date(value));
  }
  async function loadHistory() {
    historyLoading = true; historyError = '';
    try {
      const response = await fetch('/api/app-releases', { cache: 'no-store', credentials: 'omit' });
      if (!response.ok) throw new Error();
      const data = await response.json();
      if (!secureLink(data.url) || !Array.isArray(data.releases) || !data.releases.every((item: ReleaseSummary) =>
        item && typeof item.id === 'string' && /^\d+$/.test(item.id) && typeof item.tag === 'string' && typeof item.version === 'string' && typeof item.prerelease === 'boolean' && typeof item.isLatest === 'boolean' && Number.isFinite(item.size) && item.size > 0 &&
        (item.publishedAt === null || typeof item.publishedAt === 'string') && secureLink(item.url) && secureLink(item.notesUrl)
      )) throw new Error();
      history = data.releases; historyUrl = data.url;
    } catch { historyError = '버전 목록을 불러오지 못했어요.'; }
    finally { historyLoading = false; }
  }
  let loading = $state(true);
  let error = $state('');
  async function load() {
    loading = true; error = '';
    try {
      const response = await fetch('/api/app-update', { cache: 'no-store', credentials: 'omit' });
      if (response.status === 204) { release = null; return; }
      if (!response.ok) throw new Error();
      const data = await response.json();
      const url = new URL(data.url);
      if (url.protocol !== 'https:' || url.username || url.password || typeof data.version !== 'string' || !Number.isFinite(data.size) || data.size <= 0 || typeof data.notes !== 'string') throw new Error();
      release = data;
    } catch { error = '다운로드 정보를 불러오지 못했어요. 잠시 뒤 다시 확인해 주세요.'; }
    finally { loading = false; }
  }
  onMount(() => { if (iosPage) return; if (versionsPage) void loadHistory(); else void load(); });
</script>

<svelte:head><title>{iosPage ? '홍시 · iOS' : versionsPage ? '홍시 앱 버전' : '홍시 앱 다운로드'}</title><meta name="description" content={iosPage ? 'iOS 앱 배포를 위한 후원 안내와 Safari에서 홍시를 홈 화면에 추가하는 방법을 알아보세요.' : '빠른 출석 체크와 과제 일정·마감 알림을 홍시 Android 앱에서 이용하세요.'} /></svelte:head>
<main class="download-page">
  <header><a class="brand" href="/#/home"><img src="/favicon.svg" alt="" width="36" height="36" />홍시</a>{#if versionsPage || iosPage}<a class="back" href="/download">앱 다운로드 <Icon name="download" size={17} /></a>{:else}<a class="back" href="/#/home">웹에서 이용하기 <Icon name="right" size={17} /></a>{/if}</header>
  <section class:ios-page={iosPage}>
    {#if iosPage}
      <div class="ios-intro">
        <h1 class="eyebrow">홍시 · iOS</h1>
        <details class="details latest-changes ios-support">
          <summary><h2>iOS 앱 배포는 잠시 보류 중이에요.</h2><span class="latest-chevron"><Icon name="down" size={18} /></span></summary>
          <div class="ios-support-content">
            <p>앱스토어 배포에는 <a href="https://developer.apple.com/programs/" target="_blank" rel="noopener noreferrer">연간 99달러</a>의 개발자 등록비가 필요해요.</p>
            <p>후원비는 앱스토어 및 플레이스토어 앱 등록에 사용돼요.</p>
            <a class="btn btn-primary ios-donate" href="https://ko-fi.com/manbo" target="_blank" rel="noopener noreferrer"><img src="/kofi.png" alt="" width="22" height="18" />후원하러 가기 <Icon name="arrow-up-right" size={17} /></a>
          </div>
        </details>
      </div>
      <div class="ios-guide">
        <div class="ios-guide-heading"><h2>Safari에서 홈 화면에 추가하기</h2><a class="ios-web-link" href="/#/home">홍시 웹 열기 <Icon name="arrow-up-right" size={16} /></a></div>
        <p class="ios-guide-intro">아래 순서대로 추가하면 홈 화면의 홍시 아이콘으로 바로 열 수 있어요.</p>
        <ol class="ios-steps" role="list">
          {#each iosSteps as step, index (step.image)}
            <li>
              <div class="ios-step-heading"><span class="ios-step-number" aria-hidden="true">{index + 1}</span><h3>{step.title}</h3></div>
              <p>{step.text}</p>
              <a class="ios-screenshot" href={`/guides/ios/${step.image}`} target="_blank" rel="noopener noreferrer" aria-label={`${index + 1}단계 화면 크게 보기`}><img src={`/guides/ios/${step.image}`} alt={step.alt} width="1206" height="2622" loading="lazy" decoding="async" /></a>
            </li>
          {/each}
        </ol>
        <p class="ios-guide-note">iOS 버전과 설정에 따라 메뉴 위치가 조금 다를 수 있어요. 이미지를 누르면 크게 볼 수 있어요.</p>
        <details class="ios-help"><summary>홈 화면에 추가가 보이지 않아요. <Icon name="down" size={16} /></summary><p>공유 메뉴 맨 아래의 동작 편집에서 홈 화면에 추가를 켜 주세요. 카카오톡 같은 앱 안에서 열었다면 Safari로 다시 열어 주세요.</p></details>
        <a class="ios-apple-guide" href="https://support.apple.com/ko-kr/guide/iphone/iphea86e5236/ios" target="_blank" rel="noopener noreferrer">Apple의 홈 화면 추가 안내 <Icon name="arrow-up-right" size={15} /></a>
      </div>
    {:else if versionsPage}
      <p class="eyebrow">홍시 · Android</p>
      <div class="release-history">
        <div class="history-heading">
          <h1>전체 버전</h1>
          {#if historyUrl}<a class="history-all" href={historyUrl} target="_blank" rel="noopener noreferrer">전체 버전 변경사항 <Icon name="arrow-up-right" size={15} /></a>{/if}
        </div>
        <div class="history-tabs" role="tablist" aria-label="배포 종류">
          <button id="release-tab-stable" role="tab" aria-selected={releaseTab === 'stable'} aria-controls="release-panel" tabindex={releaseTab === 'stable' ? 0 : -1} onclick={() => selectReleaseTab('stable')} onkeydown={moveReleaseTab}>정식 버전</button>
          <button id="release-tab-beta" role="tab" aria-selected={releaseTab === 'beta'} aria-controls="release-panel" tabindex={releaseTab === 'beta' ? 0 : -1} onclick={() => selectReleaseTab('beta')} onkeydown={moveReleaseTab}>베타 버전</button>
        </div>
        <div id="release-panel" role="tabpanel" aria-labelledby={`release-tab-${releaseTab}`}>
        {#if historyLoading}
          <p class="history-message" role="status">버전 목록을 확인하고 있어요…</p>
        {:else if historyError}
          <div class="history-error" role="alert"><p class="history-message">{historyError}</p><button class="icon-btn" onclick={loadHistory} aria-label="버전 목록 다시 불러오기"><Icon name="refresh" size={20} /></button></div>
        {:else if visibleHistory.length}
          <ul class="release-list">
            {#each visibleHistory.slice(0, visibleCount) as item (item.id)}
              <li class="release-entry">
                <div class="release-row">
                  <button class="release-toggle" onclick={() => toggleVersion(item)} aria-expanded={expandedId === item.id} aria-controls={`release-notes-${item.id}`} aria-label={`${item.tag} 변경사항`}>
                    <span class="release-info">
                      <span class="release-title">
                        <span class="release-number">{item.tag}</span>
                        {#if item.isLatest}<span class="release-badge latest">최신 버전</span>{/if}
                        {#if item.prerelease}<span class="release-badge beta">베타</span>{/if}
                      </span>
                      <span class="release-meta">
                        {#if publishedDate(item.publishedAt)}<time datetime={item.publishedAt ?? undefined}>{publishedDate(item.publishedAt)}</time>{/if}
                        <span>{(item.size / 1024 / 1024).toFixed(1)} MB</span>
                      </span>
                    </span>
                    <span class="release-chevron" class:expanded={expandedId === item.id}><Icon name="down" size={18} /></span>
                  </button>
                  <a class="btn btn-primary release-download" href={item.url} download aria-label={`${item.tag} APK 다운로드`}><Icon name="download" size={18} />APK</a>
                </div>
                <div id={`release-notes-${item.id}`} hidden={expandedId !== item.id} class="release-change">
                  {#if expandedId === item.id}
                    {#if notes[item.id]?.loading}<p class="history-message" role="status">변경사항을 불러오고 있어요…</p>
                    {:else if notes[item.id]?.error}<div class="history-error" role="alert"><p class="history-message">{notes[item.id].error}</p><button class="icon-btn" onclick={() => loadNotes(item)} aria-label={`${item.tag} 변경사항 다시 불러오기`}><Icon name="refresh" size={20} /></button></div>
                    {:else}<p class="notes">{notes[item.id]?.text || '등록된 변경사항이 없어요.'}</p>{/if}
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
          {#if visibleHistory.length > visibleCount}<button class="history-more" onclick={() => visibleCount += 5}>이전 버전 더 보기 <Icon name="down" size={16} /></button>{/if}
        {:else}
          <div class="history-empty"><Icon name="download" size={22} /><p>아직 배포된 {releaseTab === 'stable' ? '정식' : '베타'} 버전이 없어요.</p></div>
        {/if}
        </div>
      </div>
    {:else}
      <p class="eyebrow">홍시 · Android</p>
      <h1>출석부터 과제 마감까지,<br />휴대폰에서 바로.</h1>
      <p class="intro">빠른 출석 체크와 과제 일정·마감 알림을 앱에서 이용하세요.</p>
      <div class="download-actions">
        {#if release && !loading && !error}<a class="btn btn-primary install" href={release.url} download><Icon name="download" size={21} />APK 다운로드</a>{/if}
        <button class="btn btn-ghost install play-store" type="button" disabled>
          <svg width="21" height="23" viewBox="0 0 24 26" fill="none" aria-hidden="true">
            <path d="M2 1 14 8 9 13Z" fill="#34A853" />
            <path d="M2 1C1 1.5 1 2 1 3V23C1 24 1 24.5 2 25L9 13Z" fill="#4285F4" />
            <path d="m2 25 12-7-5-5Z" fill="#EA4335" />
            <path d="m14 8 8 4c1 .5 1 1.5 0 2l-8 4-5-5Z" fill="#FBBC04" />
          </svg>
          Google Play에서 받기
        </button>
      </div>
      {#if loading}<p class="loading-status" role="status">최신 버전을 확인하고 있어요…</p>
      {:else if error}<div class="status error-status" role="alert"><span>{sentenceLines(error)}</span><button class="icon-btn" onclick={load} aria-label="다운로드 정보 다시 시도"><Icon name="refresh" size={20} /></button></div>
      {:else if release}
        <div class="version-row">
          <p class="version"><span>v{release.version} · {(release.size / 1024 / 1024).toFixed(1)} MB</span><span>· Android 7.0 이상</span></p>
          <a class="versions-link" href="/download/versions">전체 버전 <Icon name="arrow-up-right" size={16} /></a>
        </div>
        <details class="details latest-changes"><summary><h2>변경사항</h2><span class="latest-chevron"><Icon name="down" size={18} /></span></summary><p class="notes">{release.notes || '사용성과 안정성을 개선했어요.'}</p></details>
      {:else}<p class="status">앱 배포를 준비하고 있어요. 먼저 웹에서 이용할 수 있어요.</p>{/if}
      <div class="details"><h2>APK 설치 방법</h2><ol><li>APK 파일을 다운로드 한 뒤 열어주세요.</li><li>보안 경고가 뜰 경우 '세부정보 더보기' - '무시하고 설치' 순으로 진행해주세요.</li><li>설치 후에는 홍시 앱 내에서 새 버전을 확인하고 자체적으로 업데이트 할 수 있어요.</li></ol><p class="ios-status"><a class="ios-guide-link" href="/download/ios"><strong>iOS 앱은 아직 배포할 수 없어요.</strong><Icon name="arrow-up-right" size={16} /></a></p>
        <div class="support">
          <p>앱이 마음에 드신다면 후원 부탁드립니다!<br />후원 금액은 추후 앱스토어 및 플레이스토어 등록비로 사용될 예정입니다.</p>
          <a class="support-link" href="https://ko-fi.com/manbo" target="_blank" rel="noopener noreferrer"><img src="/kofi.png" alt="Ko-fi" width="22" height="18" />후원하러 가기 <Icon name="arrow-up-right" size={16} /></a>
        </div></div>
    {/if}
  </section>
  <footer>홍익대학교에서 운영하는 공식 서비스가 아닙니다.</footer>
</main>

<style>
  .download-page { max-width: 840px; margin: 0 auto; padding: 24px; }
  a { text-decoration: none; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .brand { display: flex; align-items: center; gap: 9px; color: var(--text); font-size: 23px; font-weight: 800; }
  .back { display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); font-size: 14px; }
  section { max-width: 560px; margin: 76px auto 60px; }
  .eyebrow { color: var(--primary); font-size: inherit; font-weight: 750; line-height: inherit; letter-spacing: normal; margin-bottom: 14px; }
  h1 { font-size: clamp(25px, 7.3vw, 44px); letter-spacing: -0.04em; line-height: 1.25; }
  .intro { color: var(--text-2); margin: 22px 0 28px; line-height: 1.7; }
  .download-actions { display: flex; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
  .download-actions .play-store { border: 1px solid var(--border-strong); opacity: 1; }
  .play-store svg { flex: none; }
  .install { flex: 1; justify-content: center; white-space: nowrap; min-height: 52px; padding: 14px 22px; display: inline-flex; }
  .version-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 22px; }
  .version { display: flex; flex-wrap: wrap; gap: 2px 4px; color: var(--text-3); font-size: 13px; line-height: 1.6; }
  .version span { white-space: nowrap; }
  .versions-link { flex: none; display: inline-flex; align-items: center; justify-content: flex-end; gap: 4px; min-height: 44px; margin-block: -11px; color: var(--text-2); font-size: 13px; }
  .versions-link:hover { color: var(--primary-text); }
  .status, .loading-status { line-height: 1.65; margin-bottom: 12px; text-wrap: pretty; }
  .status { padding: 20px; border: 1px dashed var(--border-strong); border-radius: var(--radius); }
  .error-status { display: flex; align-items: center; gap: 14px; }
  .error-status span { flex: 1; white-space: pre-line; }
  .error-status .icon-btn { flex: none; }
  .details { border-top: 1px solid var(--border); margin-top: 36px; padding-top: 24px; line-height: 1.8; }
  h2 { font-size: 16px; margin-bottom: 10px; }
  .latest-changes { margin-top: 16px; padding: 6px 0; }
  .latest-changes + .details { margin-top: 0; padding-top: 16px; }
  .latest-changes summary { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 44px; cursor: pointer; list-style: none; }
  .latest-changes summary::-webkit-details-marker { display: none; }
  .latest-changes summary h2 { margin: 0; }
  .latest-changes summary:hover { color: var(--primary-text); }
  .latest-chevron { display: inline-flex; color: var(--text-3); }
  .latest-changes[open] .latest-chevron { transform: rotate(180deg); }
  .latest-changes .notes { padding: 4px 0 16px; }
  .notes { color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  .history-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 16px; margin-bottom: 14px; }
  .history-heading h1 { margin: 0; font-size: 16px; letter-spacing: normal; line-height: 1.8; }
  .history-all { display: inline-flex; align-items: center; gap: 4px; color: var(--text-2); font-size: 13px; }
  .history-all { white-space: nowrap; }
  .history-all:hover { color: var(--primary-text); }
  .history-tabs { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 4px; padding: 4px; margin: 20px 0 16px; border: 1px solid var(--border); border-radius: 12px; background: var(--surface); }
  .history-tabs button { min-height: 40px; border-radius: 8px; color: var(--text-2); font-size: 14px; font-weight: 650; }
  .history-tabs button[aria-selected="true"] { background: var(--primary-weak); color: var(--primary-text); }
  .history-empty { display: flex; align-items: center; justify-content: center; gap: 10px; min-height: 112px; padding: 20px; border: 1px dashed var(--border-strong); border-radius: 12px; color: var(--text-3); font-size: 14px; }
  .release-list { list-style: none; padding: 0; margin: 0; border-bottom: 1px solid var(--border); }
  .release-row { display: grid; grid-template-columns: minmax(0, 1fr) auto 18px; align-items: center; column-gap: 16px; padding: 12px 0; }
  .release-entry { border-top: 1px solid var(--border); }
  .release-info { grid-column: 1; display: block; min-width: 0; }
  .release-toggle { grid-column: 1 / -1; grid-row: 1; display: grid; grid-template-columns: subgrid; align-items: center; min-width: 0; min-height: 44px; text-align: left; color: var(--text); }
  .release-toggle:hover { color: var(--primary-text); }
  .release-chevron { grid-column: 3; display: inline-flex; color: var(--text-3); }
  .release-chevron.expanded { transform: rotate(180deg); }
  .release-change { padding: 4px 0 16px; font-size: 14px; }
  .release-title { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 8px; }
  .release-badge { display: inline-flex; align-items: center; flex: none; border-radius: 999px; padding: 3px 8px; font-size: 11px; font-weight: 700; line-height: 1.4; white-space: nowrap; }
  .release-badge.latest { background: var(--ok-weak); color: var(--ok); }
  .release-badge.beta { background: var(--primary-weak); color: var(--primary-text); }
  .release-number { display: block; min-width: 0; font-size: 16px; font-weight: 750; line-height: 1.4; overflow-wrap: anywhere; }
  .release-meta { display: flex; flex-wrap: wrap; column-gap: 12px; color: var(--text-3); font-size: 12px; margin-top: 4px; }
  .release-meta time, .release-meta span { white-space: nowrap; }
  .release-download { grid-column: 2; grid-row: 1; position: relative; z-index: 1; display: inline-flex; align-items: center; gap: 6px; min-height: 44px; padding: 10px 14px; font-size: 13px; }
  .history-message { color: var(--text-3); font-size: 14px; }
  .history-error { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .history-more { display: flex; justify-content: center; align-items: center; gap: 6px; width: 100%; padding: 14px 0 0; color: var(--primary-text); font-size: 13px; font-weight: 650; }
  .release-history { margin-top: 32px; line-height: 1.8; }
  ol { padding-left: 20px; color: var(--text-2); }
  li { text-wrap: pretty; }
  .ios-status { font-size: 16px; color: var(--text); margin-top: 20px; }
  .ios-status strong { font-weight: 750; }
  .ios-guide-link { display: inline-flex; align-items: center; gap: 8px; min-height: 44px; color: var(--text); text-decoration: none; }
  .ios-guide-link:hover { color: var(--primary-text); }
  .ios-page { max-width: 720px; }
  .ios-intro { max-width: 560px; margin: 0 auto; }
  .ios-support { border-bottom: 1px solid var(--border); }
  .ios-support-content { padding: 4px 0 16px; }
  .ios-support p { color: var(--text-2); font-size: 14px; line-height: 1.8; word-break: keep-all; overflow-wrap: anywhere; }
  .ios-support p a { color: var(--text); font-weight: 700; text-decoration: underline; text-underline-offset: 3px; }
  .ios-donate { display: inline-flex; align-items: center; justify-content: center; gap: 8px; margin-top: 20px; min-height: 46px; }
  .ios-donate img { object-fit: contain; flex: none; }
  .ios-guide { margin-top: 48px; }
  .ios-guide-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px 16px; }
  .ios-guide-heading h2 { font-size: 20px; line-height: 1.5; margin: 0; letter-spacing: -0.025em; }
  .ios-web-link { display: inline-flex; align-items: center; gap: 4px; min-height: 44px; color: var(--primary-text); font-size: 13px; font-weight: 650; }
  .ios-guide-intro { margin-top: 8px; color: var(--text-2); font-size: 14px; line-height: 1.7; word-break: keep-all; }
  .ios-steps { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 20px; padding: 0; margin: 28px 0 0; list-style: none; }
  .ios-steps li { display: grid; grid-template-rows: auto 1fr auto; align-content: start; gap: 12px; min-width: 0; }
  .ios-step-heading { display: flex; align-items: flex-start; gap: 8px; color: var(--text); }
  .ios-step-number { display: grid; place-items: center; flex: none; width: 24px; height: 24px; border-radius: 50%; background: var(--primary-weak); color: var(--primary-text); font-size: 12px; font-weight: 750; }
  .ios-step-heading h3 { font-size: 14px; line-height: 1.65; font-weight: 750; word-break: keep-all; }
  .ios-steps li > p { color: var(--text-2); font-size: 13px; line-height: 1.7; word-break: keep-all; overflow-wrap: anywhere; }
  .ios-screenshot { display: block; border: 1px solid var(--border); border-radius: 20px; overflow: hidden; background: #fff; }
  .ios-screenshot img { display: block; width: 100%; height: auto; }
  .ios-guide-note { margin-top: 18px; color: var(--text-3); font-size: 12px; line-height: 1.7; word-break: keep-all; }
  .ios-help { margin-top: 24px; padding: 16px 0; border-block: 1px solid var(--border); font-size: 14px; }
  .ios-help summary { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 28px; color: var(--text); cursor: pointer; list-style: none; }
  .ios-help summary::-webkit-details-marker { display: none; }
  .ios-help p { padding-top: 12px; color: var(--text-2); line-height: 1.8; word-break: keep-all; }
  .ios-apple-guide { display: inline-flex; align-items: center; gap: 4px; min-height: 44px; margin-top: 8px; color: var(--text-3); font-size: 12px; }
  @media(max-width: 639px) {
    .ios-steps { grid-template-columns: 1fr; gap: 32px; }
    .ios-steps li { grid-template-rows: auto; }
    .ios-step-heading h3 { font-size: 16px; line-height: 1.5; }
    .ios-steps li > p { font-size: 14px; }
    .ios-screenshot { width: min(100%, 264px); justify-self: center; }
    .ios-donate { width: 100%; }
  }
  .support { margin-top: 20px; color: var(--text-2); font-size: 13px; }
  .support-link { display: flex; align-items: center; gap: 6px; color: var(--primary-text); font-weight: 650; margin-top: 6px; text-decoration: underline; text-underline-offset: 3px; }
  .support-link img { flex: none; object-fit: contain; }
  footer { color: var(--text-3); font-size: 12px; text-align: center; margin-bottom: 24px; }
  @media(max-width: 639px) { section { margin-top: 48px; } .download-actions { flex-direction: column; } .release-row { gap: 12px; } }
</style>
