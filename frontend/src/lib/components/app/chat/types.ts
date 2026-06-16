import type { TranscriptUsage } from '$lib/api/client';

export type ToolBlock = {
  id: string;
  name: string;
  args: unknown;
  result?: unknown;
  resultText?: string;
  resultExcalidrawScenes: ExcalidrawScene[];
  resultInlineImages: string[];
  isError: boolean;
  expanded: boolean;
};

export type ExcalidrawScene = {
  raw: string;
  svgHtml?: string;
  failed?: boolean;
};

export type MermaidScene = {
  raw: string;
  svgHtml?: string;
  failed?: boolean;
  error?: string;
};

export type ChartScene = {
  raw: string;
  svgHtml?: string;
  failed?: boolean;
  error?: string;
};

export type ChatTurn = {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  toolBlocks: ToolBlock[];
  isStreaming: boolean;
  renderedHtml: string;
  cleanedContent?: string;
  excalidrawScenes: ExcalidrawScene[];
  mermaidScenes: MermaidScene[];
  chartScenes: ChartScene[];
  inlineImages: string[];
  model?: string;
  source?: string;
  usage?: TranscriptUsage;
  durationMs?: number;
  settled?: boolean;
  systemKind?: 'note' | 'link' | 'approval';
  systemHref?: string;
  systemDetail?: string;
};

export type PtyOutputEvent = {
  type: 'session.output';
  session_id: string;
  seq: number;
  b64: string;
};

export type PrevTurn = {
  id: string;
  role: 'user' | 'assistant';
  content: string;
};
