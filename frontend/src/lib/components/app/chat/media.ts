const IMG_URL_RE =
  /(?<![[(])https?:\/\/[^\s<>"')\]]+\.(?:png|jpe?g|gif|webp|svg)(?:[?#][^\s<>"')\]]*)?/gi;
const DATA_IMG_RE = /data:image\/[a-z+]+;base64,[A-Za-z0-9+/]+=*/gi;

export function extractStandaloneImages(content: string): string[] {
  const imgs = new Set<string>();
  for (const m of content.matchAll(IMG_URL_RE)) imgs.add(m[0]);
  for (const m of content.matchAll(DATA_IMG_RE)) imgs.add(m[0]);
  return [...imgs];
}

function extractSingleImageBlock(item: unknown): string | null {
  if (!item || typeof item !== 'object') return null;
  const obj = item as Record<string, unknown>;

  if (obj.type === 'image' && obj.source && typeof obj.source === 'object') {
    const src = obj.source as Record<string, unknown>;
    if (src.type === 'base64' && src.media_type && src.data) {
      return `data:${src.media_type};base64,${src.data}`;
    }
    if (src.type === 'url' && typeof src.url === 'string') return src.url;
  }

  if (obj.type === 'image_url' && obj.image_url && typeof obj.image_url === 'object') {
    const iu = obj.image_url as Record<string, unknown>;
    if (typeof iu.url === 'string') return iu.url;
  }

  const maybeBase64 = obj.base64 ?? obj.data;
  const maybeMime = obj.mime_type ?? obj.media_type;
  if (
    typeof maybeBase64 === 'string' &&
    typeof maybeMime === 'string' &&
    maybeMime.startsWith('image/')
  ) {
    return `data:${maybeMime};base64,${maybeBase64}`;
  }

  return null;
}

export function extractToolResultImages(result: unknown): string[] {
  if (!result) return [];
  if (Array.isArray(result)) {
    const images: string[] = [];
    for (const item of result) {
      const img = extractSingleImageBlock(item);
      if (img) images.push(img);
    }
    return images;
  }
  const img = extractSingleImageBlock(result);
  return img ? [img] : [];
}

export function hasNonImageContent(result: unknown): boolean {
  if (!result || !Array.isArray(result)) return false;
  return result.some((item) => {
    if (!item || typeof item !== 'object') return true;
    const obj = item as Record<string, unknown>;
    return obj.type !== 'image' && obj.type !== 'image_url';
  });
}

export function extractToolResultTextParts(result: unknown): string[] {
  if (result == null) return [];
  if (typeof result === 'string') return [result];
  if (Array.isArray(result)) return result.flatMap(extractToolResultTextParts);
  if (typeof result === 'object') {
    const obj = result as Record<string, unknown>;
    if (obj.type === 'text' && typeof obj.text === 'string') return [obj.text];
    if (typeof obj.text === 'string') return [obj.text];
    if (obj.content !== undefined) return extractToolResultTextParts(obj.content);
    if (obj.resource && typeof obj.resource === 'object') {
      const resource = obj.resource as Record<string, unknown>;
      if (typeof resource.text === 'string') return [resource.text];
    }
  }
  return [];
}
