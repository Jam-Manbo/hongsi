export class ReadTimeoutError extends Error {
  constructor() {
    super('응답 시간이 초과됐어요.');
  }
}

export async function withReadTimeout<T>(read: (signal: AbortSignal) => Promise<T>, timeoutMs = 30_000): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const result = await read(controller.signal);
    if (controller.signal.aborted) throw new ReadTimeoutError();
    return result;
  } catch (e) {
    if (controller.signal.aborted) throw new ReadTimeoutError();
    throw e;
  } finally {
    clearTimeout(timer);
  }
}
