export class ApiError extends Error {
  constructor(public status: number, public code: string, message: string) { super(message); }
}

export type ResponseFormat = 'json' | 'html' | 'text' | 'other' | 'none' | 'native';
type Context = { method: string; path: string; format?: ResponseFormat };
const recent = new Map<string, number>();
const routeWords = new Set('api auth login logout logout-all device recover reconnect me avatar preferences push status background session renew health app-update app-releases files modules board calendar items verify state done alert alert-leads assign submission todos attendance receipts active submit course timetable notifications notices seen meals seats recent extend end'.split(' '));
const diagnosticCodes = new Set('error http_error invalid_request invalid_response unauthorized session_revoked session_expired classroom_token_expired login_rejected login_rate_limited school_reauth_required school_retry_required school_unreachable school_changed school_error server_unreachable server_error api_not_found not_found bad_request conflict account_changed db_error offline timeout native invalid_format too_large forbidden rate_limited method_not_allowed late_confirm_required attendance_result_unknown'.split(' '));
const genericCodes = new Set('error http_error invalid_request invalid_response server_error native'.split(' '));

export function errorOf(data: unknown): { code?: string; message?: string } | undefined {
  if (!data || typeof data !== 'object' || !('error' in data)) return;
  const error = data.error;
  if (!error || typeof error !== 'object') return;
  return {
    code: 'code' in error && typeof error.code === 'string' && error.code.trim() ? error.code : undefined,
    message: 'message' in error && typeof error.message === 'string' && error.message.trim() ? error.message : undefined,
  };
}

export function responseFormat(type: string | null): ResponseFormat {
  const mime = type?.split(';')[0].trim().toLowerCase();
  if (!mime) return 'none';
  if (mime === 'application/json' || mime.endsWith('+json')) return 'json';
  if (mime === 'text/html') return 'html';
  return mime.startsWith('text/') ? 'text' : 'other';
}

export function reportApiFailure(status: number, code: string, context: Context) {
  const method = /^(GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS)$/.test(context.method) ? context.method : 'OTHER';
  const path = context.path.split(/[?#]/)[0].split('/').map((part) => !part || routeWords.has(part) ? part : ':id').join('/');
  const diagnostic = { method, path, status, code: diagnosticCodes.has(code) ? code : 'other', format: context.format ?? 'none' };
  const key = JSON.stringify(diagnostic), now = Date.now();
  if (now - (recent.get(key) ?? -Infinity) < 60_000) return;
  if (recent.size >= 128) recent.delete(recent.keys().next().value!);
  recent.set(key, now);
  console.warn('[홍시 API]', diagnostic);
}

function fallbackMessage(status: number): string {
  if (status === 401) return '로그인이 필요해요.';
  if (status === 403) return '이 요청은 허용되지 않았어요.';
  if (status === 404) return '요청한 정보를 찾지 못했어요.';
  if (status === 413) return '파일이나 요청의 용량이 너무 커요.';
  if (status === 429) return '잠시 후 다시 시도해 주세요.';
  if (status >= 500) return '잠시 후 다시 시도해 주세요.';
  return '요청을 처리하지 못했어요.';
}

export function responseError(status: number, data: unknown, context: Context): ApiError {
  const error = errorOf(data);
  const result = new ApiError(status, error?.code ?? 'http_error', error?.message ?? fallbackMessage(status));
  reportApiFailure(status, result.code, context);
  return result;
}

export function invalidResponse(status: number, context: Context): ApiError {
  reportApiFailure(status, 'invalid_response', context);
  return new ApiError(status, 'invalid_response', '서버 응답을 확인하지 못했어요.');
}

export function connectionError(timeout: boolean, context: Context): ApiError {
  const error = new ApiError(0, timeout ? 'timeout' : 'offline', timeout ? '응답이 늦어지고 있어요.' : '서버에 연결하지 못했어요.');
  reportApiFailure(error.status, error.code, context);
  return error;
}

export function contextMessage(error: unknown, context: string): string {
  if (!(error instanceof ApiError) || genericCodes.has(error.code)) return context;
  return `${context} ${error.message}`;
}

export function submissionError(error: unknown, kind: 'assignment' | 'attendance'): unknown {
  if (!(error instanceof ApiError)) return error;
  if (error.status !== 0 && error.status < 500 && error.code !== 'invalid_response') return error;
  const name = kind === 'attendance' ? '출석' : '제출';
  return new ApiError(error.status, error.code, `${name} 결과를 확인하지 못했어요. ${name} 상태를 확인해 주세요.`);
}
