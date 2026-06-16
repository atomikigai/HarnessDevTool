import type { ChatTurn, PtyOutputEvent } from './types';

export function ptyTextFromEvent(ev: PtyOutputEvent): string {
  if (!ev.b64) return '';
  return cleanPtyText(decodeBase64Utf8(ev.b64));
}

export function createPtyFallbackTurn(sessionKind: string | null | undefined): ChatTurn {
  return {
    id: 'pty-fallback',
    role: 'assistant',
    content: '',
    toolBlocks: [],
    isStreaming: true,
    renderedHtml: '',
    excalidrawScenes: [],
    mermaidScenes: [],
    chartScenes: [],
    inlineImages: [],
    source: 'pty',
    model: sessionKind ? `${sessionKind} output` : 'agent output'
  };
}

function decodeBase64Utf8(value: string): string {
  try {
    const binary = atob(value);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder().decode(bytes);
  } catch {
    return '';
  }
}

function cleanPtyText(value: string): string {
  return value
    .replace(/\x1b\][^\x07]*(?:\x07|\x1b\\)/g, '')
    .replace(/\x1b\[[0-?]*[ -/]*[@-~]/g, '')
    .replace(/\x1b[=>]/g, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n')
    .replace(//g, '')
    .replace(/\n{4,}/g, '\n\n\n');
}
