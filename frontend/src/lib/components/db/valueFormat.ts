type TaggedDbValue = {
  _t: string;
  v: unknown;
};

export function isTaggedDbValue(value: unknown): value is TaggedDbValue {
  return (
    typeof value === 'object' &&
    value !== null &&
    '_t' in value &&
    'v' in value &&
    typeof (value as { _t?: unknown })._t === 'string'
  );
}

function bytesLength(base64: string): number | null {
  const clean = base64.trim();
  if (!clean) return 0;
  const padding = clean.endsWith('==') ? 2 : clean.endsWith('=') ? 1 : 0;
  const size = Math.floor((clean.length * 3) / 4) - padding;
  return Number.isFinite(size) && size >= 0 ? size : null;
}

function compact(value: string, max = 28): string {
  if (value.length <= max) return value;
  return `${value.slice(0, Math.max(0, max - 1))}...`;
}

export function editTextForDbValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (isTaggedDbValue(value)) {
    if (value._t === 'json') return JSON.stringify(value.v, null, 2);
    return value.v == null ? '' : String(value.v);
  }
  if (typeof value === 'object') return JSON.stringify(value, null, 2);
  return String(value);
}

export function formatDbValue(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (!isTaggedDbValue(value)) {
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  }

  const raw = value.v;
  switch (value._t) {
    case 'date_time':
      return raw == null ? '' : String(raw).replace('T', ' ');
    case 'date':
    case 'time':
    case 'decimal':
      return raw == null ? '' : String(raw);
    case 'bytes': {
      const base64 = raw == null ? '' : String(raw);
      const len = bytesLength(base64);
      return `bytes${len == null ? '' : ` ${len} B`} ${compact(base64)}`.trim();
    }
    case 'json':
      return JSON.stringify(raw);
    default:
      return raw == null ? '' : String(raw);
  }
}

export function titleForDbValue(value: unknown): string {
  if (isTaggedDbValue(value)) {
    return value.v == null ? '' : String(value.v);
  }
  return formatDbValue(value);
}
