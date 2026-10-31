import type { Unit } from "./api";

export const HOUR = 60 * 60 * 1000;
export const DAY = 24 * HOUR;

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  let scaled = bytes;
  let index = 0;
  while (scaled >= 1024 && index < BYTE_UNITS.length - 1) {
    scaled /= 1024;
    index += 1;
  }
  const digits = index === 0 || scaled >= 100 ? 0 : 1;
  return `${scaled.toFixed(digits)} ${BYTE_UNITS[index]}`;
}

export function formatDuration(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

export function formatCount(count: number): string {
  if (count >= 1_000_000_000) return `${(count / 1_000_000_000).toFixed(1)}B`;
  if (count >= 1_000_000) return `${(count / 1_000_000).toFixed(1)}M`;
  if (count >= 1_000) return `${Math.round(count / 1_000)}K`;
  return `${Math.round(count)}`;
}

export function formatValue(unit: Unit, value: number): string {
  switch (unit) {
    case "percent":
      return `${Math.round(value)}%`;
    case "bytes":
      return formatBytes(value);
    case "bytes_per_second":
      return `${formatBytes(value)}/s`;
    case "seconds":
      return formatDuration(value);
    case "celsius":
      return `${Math.round(value)}°C`;
    case "count":
      return formatCount(value);
    case "amount":
      return Math.round(value).toLocaleString();
    case "timestamp":
      return formatShortAgo(value, Date.now());
  }
}

export function formatClock(time: number, withSeconds: boolean): string {
  return new Date(time).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
    second: withSeconds ? "2-digit" : undefined,
  });
}

export function formatDayTime(time: number): string {
  return new Date(time).toLocaleString([], {
    weekday: "short",
    hour: "numeric",
    minute: "2-digit",
  });
}

/** For large figures in small tiles, where the full wording will not fit. */
export function formatShortAgo(time: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - time) / 1000));
  if (seconds < 5) return "now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export function formatAgo(time: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - time) / 1000));
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds} seconds ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return minutes === 1 ? "1 minute ago" : `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "1 day ago" : `${days} days ago`;
}
