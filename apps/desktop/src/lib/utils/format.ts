const KIB = 1024;
const MIB = 1024 * 1024;

export function formatAddress(value: number): string {
  return `0x${value.toString(16).toUpperCase().padStart(8, '0')}`;
}

/** Compact hexadecimal form for tables, e.g. `0x10000`. */
export function formatHex(value: number): string {
  return `0x${value.toString(16).toUpperCase()}`;
}

function trim(value: number): string {
  return value.toFixed(2).replace(/\.?0+$/, '');
}

export function formatSize(bytes: number): string {
  if (bytes >= MIB) return `${trim(bytes / MIB)} MiB`;
  if (bytes >= KIB) return `${trim(bytes / KIB)} KiB`;
  return `${bytes} B`;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const rest = Math.round(seconds % 60);
  return `${minutes} min ${rest} s`;
}

export function formatRate(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return '';
  return `${formatSize(bytesPerSecond)}/s`;
}

export function formatPercent(fraction: number): string {
  return `${Math.floor(Math.max(0, Math.min(1, fraction)) * 100)}%`;
}

export function formatTime(timestamp: number, withMillis = true): string {
  const d = new Date(timestamp);
  const pad = (n: number, len = 2) => n.toString().padStart(len, '0');
  const base = `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  return withMillis ? `${base}.${pad(d.getMilliseconds(), 3)}` : base;
}

export function formatDateTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatVidPid(vid: number, pid: number): string {
  const h = (n: number) => n.toString(16).toUpperCase().padStart(4, '0');
  return `${h(vid)}:${h(pid)}`;
}

/** File name of a path on any platform. */
export function basename(path: string): string {
  const parts = path.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function dirname(path: string): string {
  const index = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return index > 0 ? path.slice(0, index) : path;
}

export function joinPath(dir: string, name: string): string {
  const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/';
  return dir.endsWith('/') || dir.endsWith('\\') ? `${dir}${name}` : `${dir}${sep}${name}`;
}
