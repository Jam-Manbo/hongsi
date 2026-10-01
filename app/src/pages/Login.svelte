<script lang="ts">
  import { api, isApp } from '../lib/api';
  import { APP_VERSION, REPO_URL } from '../lib/about';
  import { sentenceLines } from '../lib/format';
  import { errorText } from '../lib/net.svelte';
  import { app, pref, setPref, startSession, waitForLogout } from '../lib/store.svelte';
  import Icon from '../components/Icon.svelte';
  import Sheet from '../components/Sheet.svelte';

  let id = $state(pref('last-id', ''));
  let password = $state('');
  let busy = $state(false);
  let error = $state('');
  let showPw = $state(false);
  let remember = $state(pref('remember', isApp));
  let cautionOpen = $state(false);
  let rememberInfoOpen = $state(false);

  const REMEMBER_LABEL = isApp ? '자동 로그인' : '로그인 상태 유지';


  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    if (!id.trim() || !password) {
      error = '학번과 비밀번호를 입력해 주세요';
      return;
    }
    busy = true;
    error = '';
    const studentId = id.trim().toUpperCase();
    const keepLoggedIn = remember;
    try {
      await waitForLogout();
      const res = await api.login(studentId, password, keepLoggedIn);
      password = '';
      setPref('last-id', studentId);
      setPref('remember', keepLoggedIn);
      startSession(res.profile, keepLoggedIn, studentId);
      api
        .me()
        .then((r) => {
          if (app.profile) app.profile = r.profile;
        })
        .catch(() => {});
    } catch (err) {
      error = errorText(err, '로그인하지 못했어요');
    } finally {
      busy = false;
    }
  }
</script>

<main class="login">
  <section class="panel">
    <div class="brand">
      <img src="/favicon.svg" alt="" width="56" height="56" />
      <h1>홍시</h1>
      <p class="tagline"><b>홍</b>대생의 <b>시</b>간을 효율적으로</p>
    </div>

    {#if app.notice}
      <div class="notice"><Icon name="clock" size={18} /><span class="sentence-message">{sentenceLines(app.notice)}</span></div>
    {/if}

    <form onsubmit={submit} novalidate>
      <label>
        <span>학번</span>
        <input
          bind:value={id}
          autocomplete="username"
          autocapitalize="characters"
          spellcheck="false"
          inputmode="text"
          placeholder="예: C123456"
          required
        />
      </label>
      <label>
        <span>비밀번호</span>
        <div class="pw">
          <input
            bind:value={password}
            type={showPw ? 'text' : 'password'}
            autocomplete="current-password"
            placeholder="통합 로그인 비밀번호"
            required
          />
          <button type="button" class="eye" onclick={() => (showPw = !showPw)} aria-label={showPw ? '비밀번호 숨기기' : '비밀번호 보기'}>
            <Icon name={showPw ? 'eye-off' : 'eye'} size={20} />
          </button>
        </div>
      </label>
      <div class="remember">
        <label class="check">
          <input type="checkbox" bind:checked={remember} />
          <span class="box" aria-hidden="true"><Icon name="tick" size={14} stroke={2.8} /></span>
          <span>{REMEMBER_LABEL}</span>
        </label>
        <button
          type="button"
          class="remember-info"
          aria-label={`${REMEMBER_LABEL} 안내`}
          aria-haspopup="dialog"
          aria-expanded={rememberInfoOpen}
          onclick={() => rememberInfoOpen = true}
        ><Icon name="info" size={18} /></button>
      </div>
      {#if error}<p class="error-box" role="alert"><Icon name="alert" size={18} />{error}</p>{/if}
      <button class="btn btn-primary btn-block big" disabled={busy}>
        {#if busy}<span class="spin"><Icon name="refresh" size={18} /></span> 학교 서버에 로그인하는 중…{:else}로그인{/if}
      </button>
    </form>

    <footer class="about">
      <button class="caution-link" onclick={() => cautionOpen = true} aria-label="이용 전 주의사항"><Icon name="info" size={18} />주의사항</button>
      <span class="sep" aria-hidden="true"></span>
      {#if REPO_URL}
        <a class="gh" href={REPO_URL} target="_blank" rel="noopener noreferrer" aria-label="GitHub에서 홍시 소스 코드 보기">
          {@render githubMark()}
        </a>
      {:else}
        <span class="gh" role="img" aria-label="GitHub 저장소 (곧 공개)" title="오픈소스 저장소는 곧 공개돼요">
          {@render githubMark()}
        </span>
      {/if}
      <span class="sep" aria-hidden="true"></span>
      <span>{isApp ? '앱' : '웹'} v{APP_VERSION}</span>
      {#if !isApp}<span class="sep" aria-hidden="true"></span><a href="/download">앱 다운로드</a>{/if}
    </footer>
  </section>
</main>

<Sheet bind:open={rememberInfoOpen} title={`${REMEMBER_LABEL} 안내`}>
  <div class="cautions">
    {#if isApp}
      <section>
        <h3>기기에 자동 로그인 정보를 보관해요</h3>
        <p>자동 로그인에 동의하면 학번과 비밀번호를 기기의 보안 저장소에 보관하고, 학교 세션이 만료되면 다시 로그인하는 데 사용합니다. 비밀번호는 홍시 서버에 보내거나 저장하지 않습니다.</p>
      </section>
    {:else}
      <section>
        <h3>비밀번호는 서버에 저장하지 않아요</h3>
        <p>로그인 상태 유지에 동의하면 학교 로그인 세션을 홍시 서버에 암호화해 최대 14일 보관합니다. 브라우저에는 로그인 상태를 유지하는 쿠키를 저장합니다. 비밀번호는 로그인 처리에만 사용하며 서버에 저장하지 않습니다.</p>
      </section>
    {/if}
    <section>
      <h3>백그라운드 일정 확인도 함께 켜져요</h3>
      <p>처음 자동 로그인·로그인 상태 유지를 선택하면 ‘백그라운드 일정 확인’과 ‘백그라운드 알림’이 기본으로 켜지고, 알림 권한이 없으면 권한을 요청합니다. 권한을 허용하지 않거나 연결에 실패해도 설정은 켜진 상태로 유지되며, 연결을 다시 시도합니다. 서버에는 학교 로그인 세션을 암호화해 최대 14일 보관하고, 5분마다 클래스룸 알림과 일정 변경을 확인합니다. 비밀번호는 서버에 저장하지 않습니다.</p>
      <p>앱·웹을 이용하면서 세션이 갱신되면 보관 기간도 14일로 갱신됩니다. 학교 세션이 먼저 만료되면 다시 로그인이 필요할 수 있습니다.</p>
    </section>
    <section>
      <h3>설정에서 중단할 수 있어요</h3>
      <p>‘내 정보 · 설정 → 알림 → 백그라운드 일정 확인’을 끄면 이 계정에 연결된 모든 기기의 백그라운드 알림 등록과 서버의 백그라운드용 로그인 세션을 삭제합니다. 이 기기에서 직접 끈 설정은 재로그인해도 자동으로 다시 켜지지 않습니다.</p>
      <p>{#if isApp}‘내 정보 · 설정 → 로그아웃’을 누르면 보안 저장소에 보관한 자동 로그인용 학번과 비밀번호를 삭제합니다. 다시 로그인할 때 ‘자동 로그인’을 해제하면 자동 로그인 정보를 보관하지 않습니다.{:else}‘내 정보 · 설정 → 로그아웃’을 누르면 해당 로그인 상태 유지에 보관한 서버 세션과 브라우저의 로그인 쿠키를 삭제합니다.{/if}</p>
      <p>다른 사람과 함께 쓰는 기기에서는 {isApp ? '자동 로그인을' : '로그인 상태 유지를'} 켜지 않는 것을 권장합니다.</p>
    </section>
  </div>
  {#snippet footer()}<button class="btn btn-primary btn-block" onclick={() => rememberInfoOpen = false}>확인했어요</button>{/snippet}
</Sheet>

<Sheet bind:open={cautionOpen} title="이용 전 주의사항">
  <div class="cautions">
    <section><h3>홍익대학교의 공식 서비스가 아니에요</h3><p>홍시는 개인이 개발한 비공식 캠퍼스 도우미입니다. 홍익대학교가 제작·운영하거나 보증하는 서비스가 아닙니다.</p></section>
    <section><h3>비밀번호는 홍시 서버에 저장하지 않아요</h3><p>앱은 학교에 직접 로그인합니다. 자동 로그인을 켜면 학번과 비밀번호를 기기의 보안 저장소에 보관하며, 로그아웃하면 삭제합니다. 웹은 홍시 서버를 통해 학교에 로그인하고 비밀번호는 로그인 처리에만 사용합니다.</p></section>
    <section><h3>서버에 보관하는 정보가 있어요</h3><p>할 일·완료 표시·좌석 기록 등 동기화에 필요한 정보는 서버에 저장합니다. 웹의 로그인 상태 유지를 선택하면 학교 로그인 세션을 최대 14일 암호화해 보관합니다.</p><p>처음 자동 로그인·로그인 상태 유지를 켜면 ‘백그라운드 일정 확인’과 ‘백그라운드 알림’이 기본으로 켜지고 알림 권한을 요청합니다. 권한이나 연결 문제로 실행하지 못해도 설정은 유지되며, 연결을 다시 시도합니다. 학교 세션을 암호화해 보관하며 설정에서 끌 수 있습니다. 유효한 세션으로 앱·웹을 이용할 때 백그라운드용 세션의 보관 기간을 14일로 갱신합니다. ‘내 정보 · 설정 → 알림 → 백그라운드 일정 확인’을 끄면 이 계정의 백그라운드용 세션과 모든 기기의 백그라운드 알림 등록을 삭제합니다.</p></section>
    <section><h3>최종 결과는 학교에서 확인해 주세요</h3><p>학교 시스템 변경이나 네트워크·기기 상태에 따라 정보와 알림이 누락되거나 늦어질 수 있습니다. 출석, 과제 제출, 수강 및 좌석 배정 결과는 학교의 공식 서비스에서 직접 확인해 주세요.</p><p>앱 이용 여부와 그에 따른 행동은 사용자 본인의 판단과 책임으로 결정해 주세요. 홍시의 표시와 알림은 학교의 확정 기록이나 처리 결과를 보장하지 않습니다.</p></section>
  </div>
  {#snippet footer()}<button class="btn btn-primary btn-block" onclick={() => cautionOpen = false}>확인했어요</button>{/snippet}
</Sheet>



{#snippet githubMark()}
  <svg width="20" height="20" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <path
      d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z"
    />
  </svg>
{/snippet}

<style>
  @media (max-width: 639px) { .sentence-message { white-space: pre-line; } }
  .cautions { display: grid; gap: 22px; padding-top: 8px; }
  .cautions h3 { font-size: 15px; margin-bottom: 8px; }
  .cautions p { font-size: 13px; line-height: 1.75; color: var(--text-2); }
  .cautions p + p { margin-top: 8px; }

  .login {
    position: relative;
    min-height: 100dvh;
    display: grid;
    place-items: center;
    padding: 24px 18px calc(24px + var(--safe-b));
    overflow: hidden;
  }

  .panel {
    position: relative;
    z-index: 1;
    width: min(420px, 100%);
    padding: 32px 28px 24px;
    border-radius: 24px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    display: grid;
    gap: 28px;
  }

  .brand {
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr);
    align-items: center;
    gap: 2px 14px;
    text-align: left;
  }

  .brand img {
    border-radius: 16px;
    box-shadow: var(--shadow);
    grid-row: 1 / 3;
  }

  h1 {
    font-size: 26px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }

  .brand p {
    grid-column: 2;
    color: var(--text-2);
    font-size: 13.5px;
    font-weight: 500;
    letter-spacing: -0.01em;
  }

  .tagline b {
    color: var(--persimmon);
    font-size: 1em;
    font-weight: 800;
    line-height: 1;
  }

  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    background: var(--warn-weak);
    color: var(--warn);
    font-size: 13.5px;
    font-weight: 600;
  }

  form {
    display: grid;
    gap: 14px;
  }

  label {
    display: grid;
    gap: 6px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  input {
    width: 100%;
    height: 50px;
    padding: 0 14px;
    border-radius: 12px;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    font-size: 16px;
    transition:
      border-color 0.15s,
      box-shadow 0.15s;
  }

  input:focus {
    outline: none;
    border-color: var(--primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--primary) 18%, transparent);
  }

  .pw {
    position: relative;
  }

  .pw input {
    padding-right: 52px;
  }

  .eye {
    display: grid;
    place-items: center;
    position: absolute;
    right: 8px;
    top: 50%;
    transform: translateY(-50%);
    width: 40px;
    height: 40px;
    padding: 0;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 650;
    color: var(--text-2);
  }

  .remember {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: -2px;
  }

  .remember-info {
    display: grid;
    place-items: center;
    flex: none;
    width: 40px;
    height: 40px;
    padding: 0;
    border-radius: 50%;
    color: var(--text-2);
  }

  .remember-info:hover {
    background: var(--surface-2);
    color: var(--primary);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    cursor: pointer;
  }

  .check input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
  }

  .box {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 7px;
    border: 2px solid var(--border-strong);
    color: transparent;
    transition: all 0.15s;
  }

  .check input:checked + .box {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }

  .check input:focus-visible + .box {
    outline: 2.5px solid var(--primary);
    outline-offset: 2px;
  }

  .big {
    height: 52px;
    font-size: 16px;
    margin-top: 4px;
  }

  .about {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 10px;
    margin-top: -4px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }

  .gh {
    display: inline-grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 10px;
    color: var(--text-2);
    transition:
      background 0.15s,
      color 0.15s;
  }

  a.gh:hover {
    background: var(--surface-3);
    color: var(--text);
  }

  .caution-link { display: inline-flex; align-items: center; gap: 5px; min-height: 40px; color: var(--text-2); border-radius: 8px; }
  .caution-link:hover { color: var(--primary-text); }

  .sep {
    width: 3px;
    height: 3px;
    border-radius: 999px;
    background: var(--border-strong);
  }
</style>
