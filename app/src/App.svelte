<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { watchAppUpdates } from './lib/app-update.svelte';
  import AppUpdate from './components/AppUpdate.svelte';
  import DownloadPage from './pages/DownloadPage.svelte';
  import { ApiError, api, isApp, native } from './lib/api';
  import { isPending } from './lib/colors';
  import { isTodoPending, todoDeadline } from './lib/todos.svelte';
  import { errorText } from './lib/net.svelte';
  import { syncSeatReminders } from './lib/seat.svelte';
  import { syncDueReminders } from './lib/reminders';
  import { initNotifications, notificationState, refreshNotifications, takeNotificationIntent } from './lib/notify';
  import { background, enableBackgroundByDefault, refreshBackground, syncBackgroundPreferences } from './lib/background.svelte';
  import { openNotification } from './lib/notification-navigation';
  import { seatPrefs } from './lib/seat.svelte';
  import { refreshState, refreshTab } from './lib/refresh.svelte';
  import { settings } from './lib/settings.svelte';
  import {
    app,
    attendance,
    calendar,
    endSession,
    logoutSession,
    startSession,
    lectures,
    meals,
    notices,
    seatSession,
    seats,
    timetable,
    todos,
  } from './lib/store.svelte';
  import { TABS, go, openSeats, route, toast } from './lib/ui.svelte';
  import Avatar from './components/Avatar.svelte';
  import ConnBanner from './components/ConnBanner.svelte';
  import Icon from './components/Icon.svelte';
  import Downloads from './components/Downloads.svelte';
  import Notices from './components/Notices.svelte';
  import ProfileSheet from './components/ProfileSheet.svelte';
  import PullRefresh from './components/PullRefresh.svelte';
  import Toasts from './components/Toasts.svelte';
  import AttendancePage from './pages/AttendancePage.svelte';
  import CalendarPage from './pages/CalendarPage.svelte';
  import Home from './pages/Home.svelte';
  import Login from './pages/Login.svelte';
  import MealsPage from './pages/MealsPage.svelte';
  import SeatsPage from './pages/SeatsPage.svelte';

  const TITLES = { home: '홈', calendar: '캘린더', seats: '열람실', attendance: '출결', meals: '학식' } as const;
  const downloadPage = !isApp && location.pathname.replace(/\/$/, '') === '/download';

  const weekDue = $derived.by(() => {
    const now = Date.now() / 1000;
    const school = (calendar.data?.items ?? []).filter((i) => isPending(i, now) && i.due !== null && i.due - now < 7 * 86_400).length;
    const personal = (todos.data ?? []).filter((t) => isTodoPending(t, now) && todoDeadline(t) !== null && todoDeadline(t)! - now < 7 * 86_400).length;
    return school + personal;
  });
  const attendOpen = $derived(lectures.at > Date.now() - 10 * 60_000 && (lectures.data?.items.length ?? 0) > 0);

  let profileOpen = $state(false);
  let bootError = $state('');
  let booting = false;

  async function boot() {
    if (booting) return;
    booting = true;
    try {
      const r = await api.me();
      const remembered = isApp ? await native.autoLoginEnabled().catch(() => false) : r.remembered;
      startSession(r.profile, remembered);
      bootError = '';
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        endSession();
        bootError = '';
      } else {
        bootError = errorText(e, '홍시를 열지 못했어요');
      }
    } finally {
      booting = false;
      app.booting = false;
    }
  }

  onMount(() => {
    if (downloadPage) { app.booting = false; return; }
    const stopUpdates = watchAppUpdates();
    let disposed = false;
    let cleanup = () => {};
    void initNotifications().then((fn) => { if (disposed) fn(); else cleanup = fn; });
    void boot();
    return () => { disposed = true; cleanup(); stopUpdates(); };
  });

  $effect(() => {
    if (!app.profile || !notificationState.pending) return;
    const intent = untrack(takeNotificationIntent);
    if (intent) void untrack(() => openNotification(intent));
  });

  $effect(() => {
    if (!app.profile) return;
    let last = 0;
    const refresh = async (force = true) => {
      if (document.visibilityState !== 'visible' || Date.now() - last < 30_000) return;
      last = Date.now();
      await Promise.all([calendar.load(force), todos.load(force), seatSession.load(force)]);
      await refreshNotifications();
      await refreshBackground();
    };
    void untrack(() => { void refreshBackground(true); void refresh(false); });
    const resume = () => { void refresh(); };
    window.addEventListener('focus', resume);
    window.addEventListener('online', resume);
    document.addEventListener('visibilitychange', resume);
    const tick = setInterval(resume, 5 * 60_000);
    return () => {
      clearInterval(tick);
      window.removeEventListener('focus', resume);
      window.removeEventListener('online', resume);
      document.removeEventListener('visibilitychange', resume);
    };
  });

  $effect(() => {
    if (!app.profile) return;
    settings.alertLeads; seatPrefs.alerts; background.status?.registered;
    void untrack(syncBackgroundPreferences);
  });

  $effect(() => {
    if (!app.profile) return;
    const remembered = app.remembered;
    notificationState.permission; notificationState.enabled; background.status; background.busy;
    void untrack(() => enableBackgroundByDefault(remembered));
  });

  $effect(() => {
    if (!bootError) return;
    const again = () => document.visibilityState === 'visible' && void boot();
    const t = setInterval(again, 20_000);
    window.addEventListener('online', again);
    return () => {
      clearInterval(t);
      window.removeEventListener('online', again);
    };
  });

  let topH = $state(64);

  $effect(() => {
    syncSeatReminders(app.profile ? seatSession.data?.session ?? null : null);
  });

  $effect(() => {
    const on = !!app.profile;
    syncDueReminders(
      on ? (calendar.data?.items ?? []) : [],
      on ? (todos.data ?? []) : [],
      calendar.data?.courses ?? [],
      settings.alertLeads,
    );
  });

  $effect(() => {
    if (!isApp) return;
    const onClick = (e: MouseEvent) => {
      if (e.defaultPrevented) return;
      const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
      if (!a || a.target !== '_blank' || !/^https?:/.test(a.href)) return;
      e.preventDefault();
      native.openUrl(a.href).catch(() => toast('브라우저를 열지 못했어요', 'error'));
    };
    document.addEventListener('click', onClick);
    return () => document.removeEventListener('click', onClick);
  });

  async function logout() {
    profileOpen = false;
    await logoutSession();
    toast('로그아웃했어요', 'success');
  }
</script>

<Toasts />

{#if downloadPage}
  <DownloadPage />
{:else if app.booting}
  <div class="boot" aria-busy="true"><img src="/favicon.svg" alt="홍시" width={isApp ? 128 : 64} height={isApp ? 128 : 64} /></div>
{:else if bootError}
  <main class="boot boot-error">
    <img src="/favicon.svg" alt="" width="56" height="56" />
    <strong>{bootError}</strong>
    
    <button class="btn btn-primary" onclick={() => boot()}>다시 시도</button>
  </main>
{:else if !app.profile}
  <Login />
{:else}
  {#key app.account}
  <div class="shell" class:app-mode={isApp} style:--topbar-h="{topH}px">
    <nav class="side" aria-label="주 메뉴">
      <a class="brand" href="#/home" aria-label="홍시 홈">
        <img src="/favicon.svg" alt="" width="32" height="32" />
        <strong>홍시</strong>
      </a>
      <div class="nav-list">
        {#each TABS as t (t.id)}
          {@const on = route.tab === t.id}
          <a href="#/{t.id}" onclick={() => { if (t.id === 'seats') openSeats('T'); }} class="nav" aria-current={on ? 'page' : undefined}>
            <span class="nav-ico">
              <Icon name={t.icon} size={21} stroke={on ? 2.1 : 1.8} />
              {#if t.id === 'attendance' && attendOpen}<i class="live-dot" aria-label="지금 출석 가능"></i>{/if}
            </span>
            <span class="nav-label">{t.label}</span>
            {#if t.id === 'calendar' && weekDue}<span class="count" aria-label="7일 안에 마감 {weekDue}개">{weekDue}</span>{/if}
          </a>
        {/each}
        {#if !isApp}
          <a class="nav app-download" href="/download" title="앱 다운로드">
            <span class="nav-ico"><Icon name="download" size={21} stroke={2} /></span>
            <span class="nav-label">앱 다운로드</span>
          </a>
        {/if}
      </div>
      <button class="account" onclick={() => (profileOpen = true)} aria-label="내 정보 · 설정 열기">
        <Avatar size={36} />
        <span class="acc-text">
          <strong>{app.profile.name || '내 정보'}</strong>
          <span>내 정보 · 설정</span>
        </span>
        <span class="acc-gear"><Icon name="gear" size={20} /></span>
      </button>
    </nav>

    {#snippet page()}
      <div class="content">
        <header class="topbar" bind:clientHeight={topH}>
          <h1 class="page-title" class:home={route.tab === 'home'}>
            <span class="brand-mark"><img src="/favicon.svg" alt="" width="30" height="30" />홍시</span>
            <span class="tab-name">{TITLES[route.tab]}</span>
          </h1>
          <div class="top-actions">
            <Downloads />
            <Notices />
            <button class="avatar-btn" onclick={() => (profileOpen = true)} aria-label="내 정보 · 설정 열기"><Avatar size={34} /></button>
          </div>
          <ConnBanner />
        </header>
        <main>
          {#key route.tab}
            {#if route.tab === 'home'}<Home />
            {:else if route.tab === 'calendar'}<CalendarPage />
            {:else if route.tab === 'seats'}<SeatsPage />
            {:else if route.tab === 'attendance'}<AttendancePage />
            {:else}<MealsPage />{/if}
          {/key}
        </main>
      </div>
    {/snippet}
    <div class="main-col">
      {#if isApp}
        <PullRefresh onrefresh={() => refreshTab(route.tab)} refreshing={refreshState[route.tab].busy} slow={refreshState[route.tab].slow} pageKey={route.tab}>{@render page()}</PullRefresh>
      {:else}
        <div class="scroller">{@render page()}</div>
      {/if}
    </div>

    <nav class="tabbar" aria-label="주 메뉴">
      {#each TABS as t (t.id)}
        <button class="tab" aria-current={route.tab === t.id ? 'page' : undefined} onclick={() => t.id === 'seats' ? openSeats('T') : go(t.id)}>
          <span class="nav-ico">
            <Icon name={t.icon} size={23} stroke={route.tab === t.id ? 2.1 : 1.7} />
            {#if t.id === 'attendance' && attendOpen}<i class="live-dot" aria-label="지금 출석 가능"></i>{/if}
          </span>
          <span>{t.label}</span>
        </button>
      {/each}
    </nav>
  </div>
  <ProfileSheet bind:open={profileOpen} onlogout={logout} />

  {/key}
{/if}
<AppUpdate />

<style>
  :global(html:has(.shell)),
  :global(body:has(.shell)) {
    height: 100%;
    overflow: hidden;
    overscroll-behavior: none;
  }

  .boot {
    min-height: 100dvh;
    display: grid;
    place-items: center;
  }

  .boot img {
    border-radius: 18px;
  }

  .boot-error {
    place-content: center;
    gap: 10px;
    padding: 24px;
    text-align: center;
  }

  .boot-error img {
    animation: none;
    margin-bottom: 6px;
  }

  .boot-error strong {
    font-size: 17px;
    font-weight: 750;
  }

  .shell {
    height: 100vh;
    height: 100dvh;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }

  .main-col {
    min-width: 0;
    height: 100%;
    overflow: hidden;
  }

  .scroller,
  .main-col :global(.scroller) {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: none;
    -webkit-overflow-scrolling: touch;
    scrollbar-gutter: stable;
  }

  .content {
    width: 100%;
    max-width: 1240px;
    margin: 0 auto;
    padding: 0 16px calc(var(--tabbar-h) + var(--safe-b) + 28px);
  }

  .topbar {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0 12px;
    margin: 0 -16px 8px;
    padding: calc(env(safe-area-inset-top, 0px) + 10px) 12px 10px 20px;
    background: color-mix(in srgb, var(--bg) 86%, transparent);
    backdrop-filter: saturate(1.4) blur(14px);
  }

  .page-title {
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }

  .brand-mark {
    display: none;
    align-items: center;
    gap: 8px;
  }

  .brand-mark img {
    border-radius: 9px;
  }

  .page-title.home .brand-mark {
    display: flex;
  }

  .page-title.home .tab-name {
    display: none;
  }

  .top-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .avatar-btn {
    border-radius: 999px;
    padding: 3px;
  }

  .tabbar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 30;
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    height: calc(var(--tabbar-h) + var(--safe-b));
    padding-bottom: var(--safe-b);
    background: color-mix(in srgb, var(--surface) 92%, transparent);
    backdrop-filter: saturate(1.4) blur(16px);
    border-top: 1px solid var(--border);
  }

  .tab {
    display: grid;
    place-items: center;
    align-content: center;
    gap: 2px;
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 650;
    transition: color 0.15s;
  }

  .tab[aria-current='page'] {
    color: var(--primary-text);
  }

  .nav-ico {
    position: relative;
    display: grid;
  }

  .live-dot {
    position: absolute;
    top: -2px;
    right: -4px;
    width: 9px;
    height: 9px;
    border-radius: 999px;
    background: var(--ok);
    box-shadow: 0 0 0 2px var(--surface);
  }

  .side {
    display: none;
  }

  .nav.app-download {
    margin-top: 12px;
    color: var(--primary-text);
    background: var(--primary-weak);
    border: 1px solid color-mix(in srgb, var(--primary) 25%, var(--border));
  }

  .nav.app-download:hover {
    background: color-mix(in srgb, var(--primary) 15%, var(--surface));
  }


  @media (min-width: 640px) {
    .shell {
      grid-template-columns: 84px minmax(0, 1fr);
    }

    .tabbar,
    .avatar-btn {
      display: none;
    }

    .content {
      padding: 0 28px 48px;
    }

    .topbar {
      margin: 0 -28px 12px;
      padding: 18px 20px 12px 28px;
    }

    .page-title {
      font-size: 24px;
    }

    .page-title.home .brand-mark {
      display: none;
    }

    .page-title.home .tab-name {
      display: inline;
    }

    .side {
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 6px;
      height: 100%;
      padding: 18px 8px 16px;
      background: var(--surface);
      border-right: 1px solid var(--border);
      overflow-y: auto;
      overscroll-behavior: none;
    }

    .brand {
      display: grid;
      place-items: center;
      margin-bottom: 14px;
      color: var(--text);
      text-decoration: none;
    }

    .brand img {
      border-radius: 10px;
      box-shadow: var(--shadow-sm);
    }

    .brand strong {
      display: none;
    }

    .nav-list {
      display: grid;
      gap: 4px;
      width: 100%;
    }

    .nav {
      position: relative;
      display: grid;
      justify-items: center;
      gap: 4px;
      padding: 10px 0 8px;
      border-radius: 14px;
      color: var(--text-3);
      font-size: 11.5px;
      font-weight: 650;
      text-decoration: none;
      transition:
        background 0.15s,
        color 0.15s;
    }

    .nav:hover {
      background: var(--surface-2);
      color: var(--text-2);
    }

    .nav[aria-current='page'] {
      background: var(--primary-weak);
      color: var(--primary-text);
    }

    .count {
      position: absolute;
      top: 4px;
      right: 10px;
      min-width: 18px;
      height: 18px;
      padding: 0 5px;
      border-radius: 999px;
      background: var(--primary);
      color: var(--on-primary);
      font-size: 10.5px;
      font-weight: 800;
      line-height: 18px;
      text-align: center;
      font-variant-numeric: tabular-nums;
    }

    .account {
      margin-top: auto;
      display: grid;
      place-items: center;
      padding: 6px;
      border-radius: 999px;
    }

    .acc-text,
    .acc-gear {
      display: none;
    }
  }

  @media (min-width: 1024px) {
    .shell {
      grid-template-columns: 224px minmax(0, 1fr);
    }

    .content {
      padding: 0 40px 20px;
    }

    .topbar {
      margin: 0 -40px 16px;
      padding: 18px 32px 14px 40px;
    }

    .page-title {
      font-size: 26px;
    }

    .side {
      align-items: stretch;
      gap: 0;
      padding: 22px 14px 14px;
    }

    .brand {
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 0 10px;
      margin-bottom: 26px;
    }

    .brand strong {
      display: inline;
      font-size: 19px;
      font-weight: 800;
      letter-spacing: -0.03em;
    }

    .nav-list {
      gap: 2px;
    }

    .nav {
      display: flex;
      align-items: center;
      justify-items: start;
      gap: 12px;
      height: 44px;
      padding: 0 12px;
      border-radius: 12px;
      color: var(--text-2);
      font-size: 14.5px;
      font-weight: 650;
    }

    .nav[aria-current='page'] {
      font-weight: 750;
    }

    .count {
      position: static;
      margin-left: auto;
      background: var(--primary);
      color: var(--on-primary);
      font-size: 11.5px;
    }


    .nav[aria-current='page'] .count {
      background: var(--primary);
      color: var(--on-primary);
    }

    .account {
      display: flex;
      align-items: center;
      gap: 10px;
      width: 100%;
      padding: 10px;
      border-radius: 14px;
      border: 1px solid var(--border);
      background: var(--surface-2);
      text-align: left;
      transition:
        background 0.15s,
        border-color 0.15s;
    }

    .account:hover {
      background: var(--surface-3);
      border-color: var(--border-strong);
    }

    .acc-text {
      flex: 1;
      min-width: 0;
      display: grid;
      line-height: 1.3;
    }

    .acc-text strong {
      font-size: 14px;
      color: var(--text);
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
    }

    .acc-text span {
      font-size: 12px;
      font-weight: 600;
      color: var(--text-3);
      font-variant-numeric: tabular-nums;
      letter-spacing: 0.02em;
    }

    .acc-gear {
      display: grid;
      flex: none;
      place-items: center;
      width: 24px;
      height: 24px;
      color: var(--text-3);
    }
  }

  .tab .nav-ico { width: 52px; height: 30px; place-items: center; border-radius: 12px; transition: background 160ms, transform 160ms; }
  .tab[aria-current='page'] .nav-ico { background: var(--primary-weak); }
  .tab:active .nav-ico { transform: scale(.92); }
  .avatar-btn { min-width: 44px; min-height: 44px; display: grid; place-items: center; }
  .tabbar { background: var(--surface); backdrop-filter: none; padding-top: 6px; }
  .page-title { letter-spacing: -.045em; }
  @media (min-width: 640px) { .avatar-btn { display: none; } }
  @media (min-width: 1024px) {
    .nav { height: 48px; }
    .nav-list { gap: 6px; }
    .account { border-color: transparent; }
  }

</style>
