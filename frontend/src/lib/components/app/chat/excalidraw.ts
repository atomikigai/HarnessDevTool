import DOMPurify from 'dompurify';
import type { ExcalidrawScene } from './types';

export function extractExcalidrawBlocks(content: string): { cleaned: string; scenes: string[] } {
  const scenes: string[] = [];
  const cleaned = content.replace(/```excalidraw\r?\n([\s\S]*?)```/g, (_match, body: string) => {
    scenes.push(body.trim());
    return '';
  });
  return { cleaned, scenes };
}

export function isExcalidrawJson(val: unknown): boolean {
  if (!val || typeof val !== 'object') return false;
  const obj = val as Record<string, unknown>;
  return (obj.type === undefined || obj.type === 'excalidraw') && Array.isArray(obj.elements);
}

export function normalizedExcalidrawScene(raw: string): string | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (isExcalidrawJson(parsed)) return JSON.stringify(parsed);
    if (Array.isArray(parsed)) {
      return JSON.stringify({
        type: 'excalidraw',
        version: 2,
        source: 'harness',
        elements: parsed,
        appState: {},
        files: null
      });
    }
  } catch {
    // Not JSON; callers keep it as regular text.
  }
  return null;
}

let excalidrawModPromise: Promise<{ exportToSvg: Function } | null> | null = null;

function getExcalidrawMod(): Promise<{ exportToSvg: Function } | null> {
  if (!excalidrawModPromise) {
    excalidrawModPromise = import('@excalidraw/utils')
      .then((m) => ({ exportToSvg: m.exportToSvg as Function }))
      .catch(() => null);
  }
  return excalidrawModPromise;
}

export async function renderExcalidraw(scene: ExcalidrawScene): Promise<void> {
  if (scene.svgHtml || scene.failed) return;
  try {
    const parsed: unknown = JSON.parse(scene.raw);
    if (!isExcalidrawJson(parsed)) {
      scene.failed = true;
      return;
    }
    const mod = await getExcalidrawMod();
    if (!mod) {
      scene.failed = true;
      return;
    }
    const parsedScene = parsed as Record<string, unknown>;
    const svgEl: SVGSVGElement = await (mod.exportToSvg as any)({
      elements: parsedScene.elements,
      appState: parsedScene.appState ?? {},
      files: parsedScene.files ?? null
    });
    scene.svgHtml = DOMPurify.sanitize(svgEl.outerHTML, {
      USE_PROFILES: { svg: true, svgFilters: true }
    });
  } catch {
    scene.failed = true;
  }
}
