import {
  extractExcalidrawBlocks,
  isExcalidrawJson,
  normalizedExcalidrawScene,
  renderExcalidraw
} from './excalidraw';
import { extractStandaloneImages, extractToolResultTextParts } from './media';
import type { ExcalidrawScene, ToolBlock } from './types';

export function hydrateToolResult(block: ToolBlock): void {
  const textParts = extractToolResultTextParts(block.result);
  const scenes: ExcalidrawScene[] = [];
  const visibleText: string[] = [];

  const directScene =
    typeof block.result === 'string'
      ? normalizedExcalidrawScene(block.result.trim())
      : isExcalidrawJson(block.result)
        ? JSON.stringify(block.result)
        : null;

  if (directScene) {
    scenes.push({ raw: directScene });
  }

  for (const part of textParts) {
    const { cleaned, scenes: fencedScenes } = extractExcalidrawBlocks(part);
    for (const rawScene of fencedScenes) {
      scenes.push({ raw: normalizedExcalidrawScene(rawScene) ?? rawScene });
    }

    const trimmed = cleaned.trim();
    const scene = trimmed ? normalizedExcalidrawScene(trimmed) : null;
    if (scene) {
      scenes.push({ raw: scene });
    } else if (trimmed) {
      visibleText.push(cleaned);
    }
  }

  block.resultText = visibleText.join('\n\n').trim();
  block.resultExcalidrawScenes = scenes;
  block.resultInlineImages = block.resultText ? extractStandaloneImages(block.resultText) : [];

  for (const scene of block.resultExcalidrawScenes) {
    void renderExcalidraw(scene);
  }
}
