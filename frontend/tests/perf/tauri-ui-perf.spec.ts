import { expect, test, type Page, type Route } from '@playwright/test';

const session = {
  id: 's-perf-1',
  kind: 'codex',
  thread_id: 't-perf',
  cwd: '/tmp/harness-perf',
  pid: 1234,
  status: 'running',
  started_at: new Date().toISOString(),
  exit_code: null,
  role: 'generator',
  owner_session_id: null,
  task_id: 'TASK-PERF',
  scopes: [],
  repo: null,
  loaded_capabilities: { mcp_servers: [], skills: [], tool_groups: [] },
  parent_session_id: null,
  root_session_id: 's-perf-1',
  detected_state: 'idle',
  has_transcript: true
};

const readiness = {
  status: 'ready',
  checked_at: Date.now(),
  cwd: '/tmp/harness-perf',
  blocking: [],
  warnings: [],
  facts: {},
  suggested_execution_mode: 'standard'
};

test('Tauri UI baseline performance', async ({ page }) => {
  test.setTimeout(45_000);
  await installPerfMocks(page);

  const navStart = performance.now();
  await page.goto('/');
  await expect(page.getByRole('button', { name: 'Chat', exact: true })).toBeVisible();
  const dashboardReadyMs = Math.round(performance.now() - navStart);

  const chatStart = performance.now();
  await page.getByRole('button', { name: 'Chat', exact: true }).click();
  await expect(page.getByText('Perf result 119')).toBeVisible();
  await expect(page.locator('code').filter({ hasText: 'const n = 119;' })).toBeVisible();
  const chatReplayMs = Math.round(performance.now() - chatStart);

  const contextStart = performance.now();
  await page.getByRole('button', { name: /^Info$/ }).click();
  await page.getByPlaceholder('Search checkpoints').fill('markdown');
  await page.getByRole('button', { name: 'Search' }).click();
  await expect(page.getByText('checkpoint hit 19', { exact: true })).toBeVisible();
  const contextSearchMs = Math.round(performance.now() - contextStart);

  const terminalStart = performance.now();
  await page.getByRole('button', { name: 'Terminal', exact: true }).click();
  await expect(page.getByRole('button', { name: /Active codex/ })).toBeVisible();
  const terminalMountMs = Math.round(performance.now() - terminalStart);

  const browserMetrics = await page.evaluate(() => {
    const memory = (
      performance as Performance & {
        memory?: { usedJSHeapSize: number; totalJSHeapSize: number; jsHeapSizeLimit: number };
      }
    ).memory;
    return {
      domNodes: document.getElementsByTagName('*').length,
      usedJSHeapMB: memory ? Math.round(memory.usedJSHeapSize / 1024 / 1024) : null,
      totalJSHeapMB: memory ? Math.round(memory.totalJSHeapSize / 1024 / 1024) : null
    };
  });

  console.log(
    `PERF ${JSON.stringify({
      dashboardReadyMs,
      chatReplayMs,
      contextSearchMs,
      terminalMountMs,
      transcriptTurns: 120,
      contextHits: 20,
      ...browserMetrics
    })}`
  );
});

async function installPerfMocks(page: Page) {
  await page.route(
    (url) => url.pathname === '/api' || url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace(/^\/api/, '');
      const method = route.request().method();

      if (path === '/health') return json(route, { version: 'perf', uptime_s: 1 });
      if (path === '/profiles/active')
        return json(route, { active: 'default', reload_required: false });
      if (path === '/threads') {
        return json(route, [
          {
            id: 't-perf',
            title: 'Perf Thread',
            created_at: new Date().toISOString(),
            execution_mode: 'standard',
            autonomy_profile: 'assisted',
            repo: null,
            readiness,
            sessions: [session]
          }
        ]);
      }
      if (path === '/threads/t-perf/tasks') return json(route, []);
      if (path === '/sessions/s-perf-1/children') return json(route, []);
      if (path === '/sessions/s-perf-1/metrics') {
        return json(route, {
          session_id: session.id,
          thread_id: session.thread_id,
          kind: session.kind,
          model: 'gpt-5.5',
          prompt_tokens: 12000,
          output_tokens: 8000,
          cache_read_tokens: 0,
          cache_write_5m_tokens: 0,
          cache_write_1h_tokens: 0,
          cost_usd: 0,
          tool_call_count: 0,
          tool_call_breakdown: {},
          loaded_capabilities: session.loaded_capabilities,
          observed_at: new Date().toISOString()
        });
      }
      if (path === '/sessions/s-perf-1/context') {
        return json(route, {
          session_id: session.id,
          thread_id: session.thread_id,
          task_id: session.task_id,
          role: session.role,
          latest_event_type: 'session.context.checkpoint_saved',
          latest_event_at: Date.now(),
          checkpoint_requested_at: Date.now() - 1000,
          checkpoint_saved_at: Date.now(),
          clear_pending_at: null,
          clear_deferred_at: null,
          clear_recommended_at: null,
          cleared_at: null,
          pressure: 0.39,
          context_tokens: 78000,
          max_context_tokens: 200000,
          model: 'gpt-5.5',
          checkpoint_preview: 'CONTEXT CHECKPOINT next_action: keep profiling markdown',
          checkpoint_structured: { next_action: 'keep profiling markdown' },
          indexed_events: 240
        });
      }
      if (path === '/sessions/s-perf-1/context/search') {
        return json(route, {
          query: url.searchParams.get('q') ?? '',
          hits: Array.from({ length: 20 }, (_, i) => ({
            thread_id: 't-perf',
            session_id: 's-perf-1',
            event_type: 'session.context.checkpoint_saved',
            at: Date.now() - i * 1000,
            pressure: 0.35 + i / 1000,
            model: 'gpt-5.5',
            snippet: `checkpoint hit ${i}`
          }))
        });
      }
      if (
        (path === '/sessions/s-perf-1/context/checkpoint' ||
          path === '/sessions/s-perf-1/context/clear') &&
        method === 'POST'
      ) {
        return json(route, { status: 'ok', reason: null });
      }
      if (path === '/sessions/s-perf-1/transcript') return sse(route, transcriptEvents());
      if (path.startsWith('/events')) return sse(route, []);

      return json(route, {});
    }
  );
}

function transcriptEvents() {
  return Array.from({ length: 120 }, (_, i) => ({
    seq: i + 1,
    session_id: 's-perf-1',
    ts: new Date().toISOString(),
    source: 'codex',
    kind: 'message',
    role: i % 2 === 0 ? 'user' : 'assistant',
    content:
      i % 2 === 0
        ? `profile turn ${i}`
        : `### Perf result ${i}\n\n- **bold** item ${i}\n- table-ish data ${i}\n\n\`\`\`ts\nconst n = ${i};\n\`\`\``,
    tool_name: null,
    tool_args: null,
    tool_use_id: null,
    tool_result: null,
    is_error: null,
    model: i % 2 === 0 ? null : 'gpt-5.5',
    usage: null,
    subtype: null,
    raw: null
  }));
}

async function json(route: Route, body: unknown) {
  await route.fulfill({
    status: 200,
    contentType: 'application/json',
    headers: { 'X-Protocol-Version': '1.0' },
    body: JSON.stringify(body)
  });
}

async function sse(route: Route, events: unknown[]) {
  await route.fulfill({
    status: 200,
    contentType: 'text/event-stream',
    headers: { 'X-Protocol-Version': '1.0' },
    body: events.map((event) => `event: transcript\ndata: ${JSON.stringify(event)}\n\n`).join('')
  });
}
