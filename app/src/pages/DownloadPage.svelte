<script lang="ts">
  import { onMount } from 'svelte';
  import { sentenceLines } from '../lib/format';
  import Icon from '../components/Icon.svelte';
  type Release = { version: string; versionCode: number; url: string; size: number; notes: string; sha256: string };
  let release = $state<Release | null>(null);
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
  onMount(() => { void load(); });
</script>

<svelte:head><title>홍시 앱 다운로드</title><meta name="description" content="빠른 출석 체크와 과제 일정·마감 알림을 홍시 Android 앱에서 이용하세요." /></svelte:head>
<main class="download-page">
  <header><a class="brand" href="/#/home"><img src="/favicon.svg" alt="" width="36" height="36" />홍시</a><a class="back" href="/#/home">웹에서 이용하기 <Icon name="right" size={17} /></a></header>
  <section>
    <img class="app-icon" src="/favicon.svg" alt="홍시" width="88" height="88" />
    <p class="eyebrow">홍시 · Android</p>
    <h1>출석부터 마감까지,<br />휴대폰에서 바로.</h1>
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
      <p class="version">v{release.version} · {(release.size / 1024 / 1024).toFixed(1)} MB · Android 7.0 이상</p>
      <div class="details"><h2>이번 업데이트</h2><p class="notes">{release.notes || '사용성과 안정성을 개선했어요.'}</p></div>
    {:else}<p class="status">앱 배포를 준비하고 있어요. 먼저 웹에서 이용할 수 있어요.</p>{/if}
    <div class="details"><h2>APK 직접 설치 방법</h2><ol><li>APK를 다운로드한 뒤 파일을 열어 주세요.</li><li>Android의 설치 안내를 확인하고 설치해 주세요.</li><li>설치 후에는 홍시 앱에서 새 버전을 확인할 수 있어요.</li></ol><p class="muted small">iOS 앱은 아직 배포하지 않아요.</p>
      <div class="support">
        <p>앱이 마음에 드신다면 후원 부탁드립니다! 후원 금액은 추후 앱스토어 및 플레이스토어 등록비로 사용될 예정입니다.</p>
        <a class="support-link" href="https://ko-fi.com/manbo" target="_blank" rel="noopener noreferrer"><img src="/kofi.png" alt="Ko-fi" width="22" height="18" />후원하러 가기 <Icon name="arrow-up-right" size={16} /></a>
      </div></div>
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
  .app-icon { border-radius: 24px; margin-bottom: 28px; }
  .eyebrow { color: var(--primary); font-weight: 750; margin-bottom: 14px; }
  h1 { font-size: clamp(30px, 6vw, 44px); letter-spacing: -0.04em; line-height: 1.25; }
  .intro { color: var(--text-2); margin: 22px 0 28px; line-height: 1.7; }
  .download-actions { display: flex; flex-wrap: wrap; gap: 12px; margin-bottom: 16px; }
  .download-actions .play-store { border: 1px solid var(--border-strong); opacity: 1; }
  .play-store svg { flex: none; }
  .install { flex: 1; justify-content: center; white-space: nowrap; min-height: 52px; padding: 14px 22px; display: inline-flex; }
  .version { color: var(--text-3); font-size: 13px; margin-top: 12px; }
  .status, .loading-status { line-height: 1.65; margin-bottom: 12px; text-wrap: pretty; }
  .status { padding: 20px; border: 1px dashed var(--border-strong); border-radius: var(--radius); }
  .error-status { display: flex; align-items: center; gap: 14px; }
  .error-status span { flex: 1; white-space: pre-line; }
  .error-status .icon-btn { flex: none; }
  .details { border-top: 1px solid var(--border); margin-top: 36px; padding-top: 24px; line-height: 1.8; }
  h2 { font-size: 16px; margin-bottom: 10px; }
  .notes { color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  ol { padding-left: 20px; color: var(--text-2); }
  li { text-wrap: pretty; }
  .small { font-size: 13px; margin-top: 10px; }
  .support { margin-top: 20px; color: var(--text-2); font-size: 13px; }
  .support-link { display: flex; align-items: center; gap: 6px; color: var(--primary-text); font-weight: 650; margin-top: 6px; text-decoration: underline; text-underline-offset: 3px; }
  .support-link img { flex: none; object-fit: contain; }
  footer { color: var(--text-3); font-size: 12px; text-align: center; margin-bottom: 24px; }
  @media(max-width: 639px) { section { margin-top: 48px; } .download-actions { flex-direction: column; } }
</style>
