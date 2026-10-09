export function studentCardRemaining(validForMs: number, processingMs: number, elapsedMs: number): number {
  if (![validForMs, processingMs, elapsedMs].every(Number.isFinite)
      || validForMs < 0 || processingMs < 0 || elapsedMs < 0) return 0;
  return Math.max(0, validForMs - Math.max(0, elapsedMs - processingMs));
}
