import type { TranscriptUsage } from '$lib/api/client';
import type { ToolBlock } from './types';

export function formatDuration(ms: number): string {
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60000);
  const s = Math.round((ms % 60000) / 1000);
  return `${m}m ${s}s`;
}

export function thinkingTail(thinking: string): string {
  const lines = thinking.split('\n');
  return lines.length <= 10 ? thinking : lines.slice(lines.length - 10).join('\n');
}

export function prettyJson(val: unknown): string {
  try {
    return JSON.stringify(val, null, 2);
  } catch {
    return String(val);
  }
}

export function formatInt(value: number | null | undefined): string {
  return new Intl.NumberFormat().format(value ?? 0);
}

export function usageLabel(usage: TranscriptUsage | undefined): string | null {
  if (!usage) return null;
  const parts: string[] = [];
  if (usage.input_tokens != null) parts.push(`${formatInt(usage.input_tokens)} in`);
  if (usage.output_tokens != null) parts.push(`${formatInt(usage.output_tokens)} out`);
  return parts.length > 0 ? parts.join(' · ') : null;
}

export function toolState(block: ToolBlock): 'error' | 'done' | 'running' {
  if (block.isError) return 'error';
  if (block.result !== undefined) return 'done';
  return 'running';
}

export function toolPreview(value: unknown): string {
  if (value == null) return '';
  if (typeof value === 'string') return value.slice(0, 140);
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? '' : 's'}`;
  if (typeof value === 'object') return Object.keys(value as Record<string, unknown>).join(', ');
  return String(value);
}

export function isImageMime(mime: string): boolean {
  return mime.startsWith('image/');
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function fileIconName(mime: string): 'json' | 'spreadsheet' | 'code' | 'text' {
  if (mime === 'application/json') return 'json';
  if (mime.includes('spreadsheet') || mime === 'text/csv' || mime.includes('excel')) {
    return 'spreadsheet';
  }
  if (mime.includes('zip') || mime.includes('tar') || mime.includes('gzip')) return 'code';
  return 'text';
}
