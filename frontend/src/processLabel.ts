export function formatProcessLabel(processName: string, processPid: number | null | undefined): string {
  const pid = processPid == null ? "?" : String(processPid);
  return `${processName}(${pid})`;
}
