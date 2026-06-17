import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { isTauri, invokeCommand } from '$lib/tauri';

const PURIFY_CFG = {
  ALLOWED_URI_REGEXP:
    /^(?:(?:(?:f|ht)tps?|mailto|tel|callto|sms|cid|xmpp):|data:image\/[a-z+]+;base64,|[^a-z]|[a-z+.\-]+(?:[^a-z+.\-:]|$))/i
};

type KatexModule = typeof import('katex');

let katexPromise: Promise<KatexModule | null> | null = null;

function getKatex(): Promise<KatexModule | null> {
  if (!katexPromise) {
    katexPromise = import('katex').catch(() => null);
  }
  return katexPromise;
}

async function renderMathMarkdown(text: string): Promise<string> {
  const katex = await getKatex();
  if (!katex) return text;

  const codeFences: string[] = [];
  const protectedText = text.replace(/```[\s\S]*?```|`[^`\n]*`/g, (match) => {
    const idx = codeFences.push(match) - 1;
    return `@@HARNESS_CODE_${idx}@@`;
  });

  const render = (expr: string, displayMode: boolean): string => {
    try {
      return katex.renderToString(expr.trim(), {
        displayMode,
        throwOnError: false,
        strict: false,
        trust: false
      });
    } catch {
      return displayMode ? `$$${expr}$$` : `$${expr}$`;
    }
  };

  const withBlocks = protectedText.replace(/\$\$([\s\S]{1,4000}?)\$\$/g, (_m, expr: string) =>
    render(expr, true)
  );
  const withBracketBlocks = withBlocks.replace(/\\\[([\s\S]{1,4000}?)\\\]/g, (_m, expr: string) =>
    render(expr, true)
  );
  const withParenInline = withBracketBlocks.replace(
    /\\\(([\s\S]{1,500}?)\\\)/g,
    (_m, expr: string) => render(expr, false)
  );
  const withInline = withParenInline.replace(
    /(^|[^\\$])\$([^\n$]{1,500}?)\$/g,
    (_m, prefix: string, expr: string) => `${prefix}${render(expr, false)}`
  );

  return withInline.replace(
    /@@HARNESS_CODE_(\d+)@@/g,
    (_m, idx: string) => codeFences[Number(idx)] ?? ''
  );
}

export async function renderMarkdown(text: string): Promise<string> {
  const mathReady = await renderMathMarkdown(text);
  if (isTauri) {
    const html = await invokeCommand<string>('parse_markdown', { text: mathReady });
    return sanitizeMarkdownHtml(html);
  }
  const html = marked.parse(mathReady, { breaks: true, gfm: true });
  return sanitizeMarkdownHtml(typeof html === 'string' ? html : '');
}

export async function renderMarkdownBatch(texts: string[]): Promise<string[]> {
  if (texts.length > 1 && isTauri) {
    const htmls = await invokeCommand<string[]>('parse_markdown_batch', { texts });
    return htmls.map(sanitizeMarkdownHtml);
  }
  return Promise.all(texts.map(renderMarkdown));
}

function sanitizeMarkdownHtml(html: string): string {
  return DOMPurify.sanitize(html, PURIFY_CFG);
}

type HljsCore = {
  highlightElement: (el: HTMLElement) => void;
  registerLanguage: (name: string, lang: unknown) => void;
};

let hljsPromise: Promise<HljsCore> | null = null;

function getHljs(): Promise<HljsCore> {
  if (!hljsPromise) {
    hljsPromise = (async () => {
      const { default: core } = await import('highlight.js/lib/core');
      const [js, ts, rust, python, bash, json, xml, css, sql] = await Promise.all([
        import('highlight.js/lib/languages/javascript'),
        import('highlight.js/lib/languages/typescript'),
        import('highlight.js/lib/languages/rust'),
        import('highlight.js/lib/languages/python'),
        import('highlight.js/lib/languages/bash'),
        import('highlight.js/lib/languages/json'),
        import('highlight.js/lib/languages/xml'),
        import('highlight.js/lib/languages/css'),
        import('highlight.js/lib/languages/sql')
      ]);
      core.registerLanguage('javascript', js.default);
      core.registerLanguage('js', js.default);
      core.registerLanguage('typescript', ts.default);
      core.registerLanguage('ts', ts.default);
      core.registerLanguage('rust', rust.default);
      core.registerLanguage('python', python.default);
      core.registerLanguage('py', python.default);
      core.registerLanguage('bash', bash.default);
      core.registerLanguage('sh', bash.default);
      core.registerLanguage('shell', bash.default);
      core.registerLanguage('json', json.default);
      core.registerLanguage('html', xml.default);
      core.registerLanguage('xml', xml.default);
      core.registerLanguage('css', css.default);
      core.registerLanguage('sql', sql.default);
      return core as HljsCore;
    })();
  }
  return hljsPromise;
}

export function highlightRenderedMarkdown(
  node: HTMLElement,
  openLightbox: (src: string) => void
): { destroy: () => void } {
  getHljs()
    .then((hljs) => {
      node.querySelectorAll('pre code').forEach((block) => {
        const el = block as HTMLElement;
        if (!el.dataset.highlighted) hljs.highlightElement(el);
      });
    })
    .catch(() => {});

  node.querySelectorAll('a[href]').forEach((anchor) => {
    const href = (anchor as HTMLAnchorElement).href;
    if (isExternalHttpUrl(href)) {
      (anchor as HTMLAnchorElement).target = '_blank';
      (anchor as HTMLAnchorElement).rel = 'noopener noreferrer';
    }
  });

  function handleContentClick(ev: MouseEvent) {
    const target = ev.target as HTMLElement;
    const anchor = target.closest('a[href]') as HTMLAnchorElement | null;
    if (anchor && isExcalidrawUrl(anchor.href)) {
      ev.preventDefault();
      ev.stopPropagation();
      window.open(anchor.href, '_blank', 'noopener,noreferrer');
      return;
    }
    if (target.tagName === 'IMG') {
      const src = (target as HTMLImageElement).src;
      if (src) openLightbox(src);
    }
  }

  node.addEventListener('click', handleContentClick);
  return { destroy: () => node.removeEventListener('click', handleContentClick) };
}

function isExternalHttpUrl(href: string): boolean {
  try {
    const url = new URL(href, window.location.href);
    return url.protocol === 'http:' || url.protocol === 'https:';
  } catch {
    return false;
  }
}

function isExcalidrawUrl(href: string): boolean {
  try {
    const url = new URL(href, window.location.href);
    return url.hostname === 'excalidraw.com' || url.hostname.endsWith('.excalidraw.com');
  } catch {
    return false;
  }
}
