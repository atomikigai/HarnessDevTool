import DOMPurify from 'dompurify';
import type { ChartScene, MermaidScene } from './types';

export function extractMermaidBlocks(content: string): { cleaned: string; scenes: string[] } {
  const scenes: string[] = [];
  const cleaned = content.replace(
    /```(?:mermaid|mmd)\r?\n([\s\S]*?)```/gi,
    (_match, body: string) => {
      scenes.push(body.trim());
      return '';
    }
  );
  return { cleaned, scenes };
}

export function extractChartBlocks(content: string): { cleaned: string; scenes: string[] } {
  const scenes: string[] = [];
  const cleaned = content.replace(
    /```(?:chart|chart-json|harness-chart)\r?\n([\s\S]*?)```/gi,
    (_match, body: string) => {
      scenes.push(body.trim());
      return '';
    }
  );
  return { cleaned, scenes };
}

let mermaidModPromise: Promise<typeof import('mermaid').default | null> | null = null;

function getMermaidMod(): Promise<typeof import('mermaid').default | null> {
  if (!mermaidModPromise) {
    mermaidModPromise = import('mermaid')
      .then((m) => {
        m.default.initialize({
          startOnLoad: false,
          securityLevel: 'strict',
          theme: 'base',
          themeVariables: {
            background: '#ffffff',
            primaryColor: '#faf8f2',
            primaryBorderColor: '#e2ddd4',
            primaryTextColor: '#2e2a22',
            lineColor: '#8a8278',
            fontFamily: 'Inter, ui-sans-serif, system-ui'
          }
        });
        return m.default;
      })
      .catch(() => null);
  }
  return mermaidModPromise;
}

export async function renderMermaid(scene: MermaidScene): Promise<void> {
  if (scene.svgHtml !== undefined || scene.failed) return;
  try {
    const mermaid = await getMermaidMod();
    if (!mermaid) {
      scene.failed = true;
      return;
    }
    const id = `harness-mermaid-${Math.random().toString(36).slice(2)}`;
    const rendered = await mermaid.render(id, scene.raw);
    scene.svgHtml = DOMPurify.sanitize(rendered.svg, {
      USE_PROFILES: { svg: true, svgFilters: true }
    });
  } catch (err) {
    scene.failed = true;
    scene.error = err instanceof Error ? err.message : String(err);
  }
}

export function renderSimpleChart(raw: string): ChartScene {
  const scene: ChartScene = { raw };
  try {
    const parsed = JSON.parse(raw) as {
      title?: string;
      labels?: string[];
      values?: number[];
      data?: Array<{ label?: string; name?: string; value?: number }>;
    };
    const points = Array.isArray(parsed.data)
      ? parsed.data.map((d) => ({
          label: String(d.label ?? d.name ?? ''),
          value: Number(d.value ?? 0)
        }))
      : (parsed.labels ?? []).map((label, i) => ({
          label,
          value: Number(parsed.values?.[i] ?? 0)
        }));
    const valid = points.filter((p) => Number.isFinite(p.value));
    if (!valid.length) throw new Error('chart has no numeric values');

    const max = Math.max(...valid.map((p) => Math.abs(p.value)), 1);
    const width = 720;
    const height = Math.max(220, valid.length * 34 + 70);
    const labelW = 150;
    const barW = width - labelW - 70;
    const rows = valid
      .map((p, i) => {
        const y = 52 + i * 34;
        const w = Math.max(2, (Math.abs(p.value) / max) * barW);
        const label = escapeHtml(p.label || `Item ${i + 1}`);
        const value = escapeHtml(String(p.value));
        return `<text x="16" y="${y + 17}" class="chart-label">${label}</text><rect x="${labelW}" y="${y}" width="${w}" height="22" rx="4" class="chart-bar"/><text x="${labelW + w + 8}" y="${y + 16}" class="chart-value">${value}</text>`;
      })
      .join('');
    const title = parsed.title
      ? `<text x="16" y="26" class="chart-title">${escapeHtml(parsed.title)}</text>`
      : '';
    const svg = `<svg viewBox="0 0 ${width} ${height}" role="img" xmlns="http://www.w3.org/2000/svg"><style>.chart-title{font:600 16px Inter,system-ui;fill:#2e2a22}.chart-label{font:12px Inter,system-ui;fill:#6b6258}.chart-value{font:12px ui-monospace,monospace;fill:#6b6258}.chart-bar{fill:#0e7864}</style>${title}${rows}</svg>`;
    scene.svgHtml = DOMPurify.sanitize(svg, { USE_PROFILES: { svg: true, svgFilters: true } });
  } catch (err) {
    scene.failed = true;
    scene.error = err instanceof Error ? err.message : String(err);
  }
  return scene;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}
