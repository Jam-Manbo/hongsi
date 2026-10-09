import { ApiError, api, onSessionRevoked, onSchoolAuthRequired } from '../../shared/api/api';
import { clearUserData, isStaleSession, setSessionUser } from '../../shared/state/session';
import type { Profile } from '../../shared/types';

export const app = $state({
  profile: null as Profile | null,
  account: null as string | null,
  booting: true,
  notice: '',
  remembered: false,
  loggingOut: false,
});

let logoutPending: Promise<void> | null = null;
const logoutPreparations = new Set<() => Promise<void>>();

export const waitForLogout = () => logoutPending ?? Promise.resolve();
export function onBeforeLogout(prepare: () => Promise<void>) {
  logoutPreparations.add(prepare);
  return () => logoutPreparations.delete(prepare);
}

export function startSession(profile: Profile, remembered: boolean, id = profile.studentId) {
  app.account = id.trim().toUpperCase() || null;
  setSessionUser(app.account);
  app.notice = '';
  app.remembered = remembered;
  app.profile = profile;
}

export function endSession() {
  clearUserData();
  app.profile = null;
  app.account = null;
  app.remembered = false;
  setSessionUser(null);
}

export function logoutSession(scope: 'device' | 'all' = 'device') {
  if (logoutPending) return logoutPending;
  app.loggingOut = true;
  let revoked = false;
  const pending = (async () => {
    await Promise.all([...logoutPreparations].map((prepare) => prepare()));
    const result = await (scope === 'all' ? api.logoutAll() : api.logout());
    if (result.ok !== true) throw new ApiError(200, 'logout_incomplete', '로그인 정보 삭제를 확인하지 못했어요. 다시 시도해 주세요.');
    endSession();
  })().catch((e) => {
    revoked = e instanceof ApiError && e.status === 401 && e.code === 'session_revoked';
    throw e;
  }).finally(() => {
    app.loggingOut = false;
    if (logoutPending === pending) logoutPending = null;
    if (revoked) handleSessionRevoked();
  });
  logoutPending = pending;
  return pending;
}

function handleSessionRevoked() {
  if (app.loggingOut) return;
  if (app.profile || app.account) endSession();
  app.notice = '다시 로그인해 주세요.';
}
function handleSchoolSessionExpired() {
  if (!app.profile || app.loggingOut) return;
  void logoutSession().catch(() => {
    if (app.profile || app.account) endSession();
  }).then(() => {
    app.notice = '로그인이 만료됐어요.';
  });
}

onSessionRevoked(handleSessionRevoked);
onSchoolAuthRequired(handleSchoolSessionExpired);

export function handleAuthError(e: unknown): boolean {
  if (isStaleSession(e)) return true;
  if (e instanceof ApiError && ['session_expired', 'classroom_token_expired', 'school_reauth_required'].includes(e.code)) {
    handleSchoolSessionExpired();
    return true;
  }
  if (e instanceof ApiError && e.status === 401 && e.code !== 'login_rejected') {
    if (e.code === 'session_revoked') { handleSessionRevoked(); return true; }
    if (app.loggingOut) return true;
    endSession();
    app.notice = '';
    return true;
  }
  return false;
}
