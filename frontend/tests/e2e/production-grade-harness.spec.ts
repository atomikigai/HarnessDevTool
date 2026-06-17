import { expect, test, type Page, type Route } from '@playwright/test';

const now = () => new Date().toISOString();

type Session = {
  id: string;
  kind: string;
  thread_id: string;
  cwd: string;
  pid: number;
  status: 'running' | 'stopped' | 'exited';
  started_at: string;
  exit_code: number | null;
  role: string | null;
  owner_session_id: string | null;
  task_id: string | null;
  scopes: string[];
  repo: null;
  loaded_capabilities: { mcp_servers: string[]; skills: string[]; tool_groups: string[] };
  parent_session_id: string | null;
  root_session_id: string;
  detected_state: 'idle' | 'working' | 'blocked' | 'unknown' | null;
  has_transcript: boolean;
};

type TranscriptEvent = ReturnType<typeof transcript>;

function baseSession(patch: Partial<Session> = {}): Session {
  const id = patch.id ?? 's-prod-root';
  return {
    id,
    kind: 'codex',
    thread_id: 't-prod',
    cwd: '/workspace/harness',
    pid: 4321,
    status: 'running',
    started_at: now(),
    exit_code: null,
    role: null,
    owner_session_id: null,
    task_id: null,
    scopes: [],
    repo: null,
    loaded_capabilities: { mcp_servers: [], skills: [], tool_groups: [] },
    parent_session_id: null,
    root_session_id: id,
    detected_state: 'idle',
    has_transcript: true,
    ...patch
  };
}

function thread(sessions: Session[]) {
  return {
    id: 't-prod',
    title: 'Production Harness QA',
    created_at: now(),
    execution_mode: 'standard',
    autonomy_profile: 'assisted',
    repo: null,
    readiness: {
      status: 'ready',
      checked_at: Date.now(),
      cwd: '/workspace/harness',
      blocking: [],
      warnings: [],
      facts: {},
      suggested_execution_mode: 'standard'
    },
    sessions
  };
}

async function installHarnessMocks(
  page: Page,
  opts: {
    sessions?: Session[];
    transcriptEvents?: Record<string, TranscriptEvent[]>;
    healthDelayMs?: number;
  } = {}
) {
  const state = {
    sessions: opts.sessions ?? [baseSession()],
    createdThread: false,
    createRequests: [] as unknown[],
    hardDeleted: [] as string[],
    inputChunks: [] as string[],
    healthCalls: 0
  };
  const transcriptEvents = opts.transcriptEvents ?? {};

  await page.route(
    (url) => url.pathname === '/api' || url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace(/^\/api/, '');
      const method = route.request().method();

      if (path === '/health') {
        state.healthCalls += 1;
        if (opts.healthDelayMs && state.healthCalls === 1) {
          await new Promise((resolve) => setTimeout(resolve, opts.healthDelayMs));
        }
        return json(route, { version: 'prod-test', uptime_s: 90, server_cwd: '/workspace/harness' });
      }
      if (path === '/profiles/active') {
        return json(route, { active: 'default', reload_required: false });
      }
      if (path === '/approvals') return json(route, []);
      if (path === '/threads' && method === 'GET') return json(route, [thread(state.sessions)]);
      if (path === '/threads' && method === 'POST') {
        state.createdThread = true;
        return json(route, {
          id: 't-created',
          title: 'Created Thread',
          created_at: now(),
          execution_mode: 'standard',
          autonomy_profile: 'assisted',
          repo: null,
          readiness: null,
          sessions: []
        }, 201);
      }
      if (path === '/threads/t-prod/tasks' || path === '/threads/t-created/tasks') return json(route, []);
      if (path === '/threads/t-created/sessions' && method === 'POST') {
        const body = route.request().postDataJSON() as Record<string, unknown>;
        state.createRequests.push(body);
        const kind = String(body.kind ?? 'claude');
        const session = baseSession({
          id: 's-created-prod',
          kind,
          thread_id: 't-created',
          cwd: String(body.cwd ?? '/workspace/harness'),
          role: kind === 'zeus' ? 'zeus-orchestrator' : null,
          root_session_id: 's-created-prod'
        });
        state.sessions = [session, ...state.sessions];
        return json(route, { session_id: session.id }, 201);
      }

      const sessionMatch = path.match(/^\/sessions\/([^/]+)(.*)$/);
      if (sessionMatch) {
        const sid = sessionMatch[1];
        const suffix = sessionMatch[2];
        const session = state.sessions.find((s) => s.id === sid) ?? baseSession({ id: sid });

        if (suffix === '' && method === 'GET') return json(route, { ...session, zeus_roles: [] });
        if (suffix === '/children') {
          return json(route, state.sessions.filter((s) => s.parent_session_id === sid));
        }
        if (suffix === '/metrics') {
          return json(route, {
            session_id: sid,
            thread_id: session.thread_id,
            kind: session.kind,
            model: session.kind === 'claude' ? 'claude-sonnet-4-6' : 'gpt-5.5',
            prompt_tokens: sid.includes('child') ? 800 : 1200,
            output_tokens: sid.includes('child') ? 300 : 500,
            cache_read_tokens: 0,
            cache_write_5m_tokens: 0,
            cache_write_1h_tokens: 0,
            cost_usd: sid.includes('child') ? 0.04 : 0.07,
            tool_call_count: sid.includes('child') ? 6 : 2,
            tool_call_breakdown: sid.includes('child') ? { repo_read_file: 3, task_submit: 1 } : {},
            loaded_capabilities: session.loaded_capabilities,
            observed_at: now()
          });
        }
        if (suffix === '/context') {
          return json(route, {
            session_id: sid,
            thread_id: session.thread_id,
            task_id: session.task_id,
            role: session.role,
            latest_event_type: null,
            latest_event_at: null,
            checkpoint_requested_at: null,
            checkpoint_saved_at: null,
            clear_pending_at: null,
            clear_deferred_at: null,
            clear_recommended_at: null,
            cleared_at: null,
            pressure: 0.12,
            context_tokens: 24000,
            max_context_tokens: 200000,
            model: 'gpt-5.5',
            checkpoint_preview: null,
            checkpoint_structured: null,
            indexed_events: 0
          });
        }
        if (suffix === '/attach' && method === 'GET') return json(route, []);
        if (suffix === '/input' && method === 'POST') {
          state.inputChunks.push(route.request().postData() ?? '');
          return json(route, null);
        }
        if (suffix === '/hard-delete' && method === 'POST') {
          state.hardDeleted.push(sid);
          const remove = new Set([sid, ...state.sessions.filter((s) => s.root_session_id === sid).map((s) => s.id)]);
          state.sessions = state.sessions.filter((s) => !remove.has(s.id));
          return json(route, null, 204);
        }
        if (suffix.startsWith('/transcript')) {
          return sse(route, transcriptEvents[sid] ?? []);
        }
      }

      if (path.startsWith('/events')) return sse(route, [], 'session.output');
      return json(route, {});
    }
  );

  return state;
}

test('case 1: creates a session through the modal and sends the expected create contract', async ({ page }) => {
  const state = await installHarnessMocks(page, { sessions: [] });
  await page.goto('/');

  await page.getByRole('button', { name: 'Create one' }).click({ timeout: 15_000 });
  await page.getByRole('radio', { name: 'codex' }).click();
  await page.getByPlaceholder('/path/to/project').fill('/workspace/harness');
  await page.getByRole('dialog', { name: 'New session' }).getByRole('button', { name: 'Create', exact: true }).click();

  await expect(page.getByRole('button', { name: /codex · s-create/ })).toBeVisible();
  await expect.poll(() => state.createdThread).toBe(true);
  await expect.poll(() => state.createRequests.length).toBe(1);
  expect(state.createRequests[0]).toMatchObject({
    kind: 'codex',
    cwd: '/workspace/harness',
    include_project_context: true,
    capability_profile: 'auto'
  });
});

test('case 2: hard delete removes a session from the UI through the destructive endpoint', async ({ page }) => {
  const state = await installHarnessMocks(page, { sessions: [baseSession({ id: 's-delete-me' })] });
  await page.goto('/');

  await expect(page.locator('button', { hasText: 's-delete' }).first()).toBeVisible({ timeout: 10_000 });
  await page.locator('button[aria-label="Delete session s-delete"]').click({ force: true, timeout: 10_000 });
  await expect(page.getByRole('heading', { name: /Permanently delete/ })).toBeVisible();
  await page.getByRole('button', { name: 'Hard delete' }).click();

  await expect(page.getByRole('button', { name: /s-delete-me/ })).toHaveCount(0);
  await expect.poll(() => state.hardDeleted).toEqual(['s-delete-me']);
});

test('case 3: health indicator stays healthy through overlapping refreshes', async ({ page }) => {
  await installHarnessMocks(page, { healthDelayMs: 250 });
  await page.goto('/');

  await expect(page.getByText('Protocol v1.0')).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/backend down/i)).toHaveCount(0);
  await expect(page.getByText(/localhost · backend/i)).toBeVisible();
});

test('case 4: terminal-first workflow sends user input over the PTY contract', async ({ page }) => {
  const state = await installHarnessMocks(page, { sessions: [baseSession({ id: 's-no-response' })] });
  await page.goto('/');

  const composer = page.getByPlaceholder('Message or command…');
  await expect(composer).toBeEditable({ timeout: 10_000 });
  await composer.fill('Implement a production-grade fix', { timeout: 10_000 });
  await page.getByRole('button', { name: 'Send' }).click();

  await expect.poll(() => state.inputChunks.join('')).toContain('Implement a production-grade fix');
  await expect.poll(() => state.inputChunks.join('')).toContain('\r');
  await expect(composer).toHaveValue('');
});

test('case 5: Zeus tree exposes child agents and per-agent metrics in the production shell', async ({ page }) => {
  const root = baseSession({
    id: 's-zeus-root',
    kind: 'codex',
    role: 'zeus-orchestrator',
    root_session_id: 's-zeus-root',
    loaded_capabilities: { mcp_servers: ['harness'], skills: ['code-review-and-quality'], tool_groups: ['task'] }
  });
  const child = baseSession({
    id: 's-child-generator',
    kind: 'codex',
    role: 'generator',
    task_id: 'TASK-42',
    owner_session_id: 's-zeus-root',
    parent_session_id: 's-zeus-root',
    root_session_id: 's-zeus-root',
    loaded_capabilities: { mcp_servers: ['harness'], skills: ['rust-tooling'], tool_groups: ['repo'] }
  });
  await installHarnessMocks(page, {
    sessions: [root, child],
    transcriptEvents: {
      's-zeus-root': [],
      's-child-generator': []
    }
  });
  await page.goto('/');

  await expect(page.locator('button', { hasText: 's-zeus-r' }).first()).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole('button', { name: /generator TASK-42/ })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Zeus session' })).toBeVisible();
  await page.getByRole('button', { name: /generator TASK-42/ }).click();
  await expect(page.locator('header').getByTitle('s-child-generator')).toBeVisible();
  await expect(page.getByPlaceholder('Message or command…')).toBeEditable();
  await page.getByRole('button', { name: /^Info$/ }).click();
  await expect(page.getByText(/repo_read_file/)).toBeVisible();
});

function transcript(input: {
  seq: number;
  sessionId?: string;
  role: 'user' | 'assistant';
  content: string;
}) {
  return {
    seq: input.seq,
    session_id: input.sessionId ?? 's-prod-root',
    ts: now(),
    source: 'codex',
    kind: 'message',
    role: input.role,
    content: input.content,
    tool_name: null,
    tool_args: null,
    tool_use_id: null,
    tool_result: null,
    is_error: null,
    model: input.role === 'assistant' ? 'gpt-5.5' : null,
    usage: input.role === 'assistant' ? { input_tokens: 100, output_tokens: 50, total_tokens: 150 } : null,
    subtype: null,
    raw: null
  };
}

async function json(route: Route, body: unknown, status = 200) {
  await route.fulfill({
    status,
    contentType: 'application/json',
    headers: { 'X-Protocol-Version': '1.0' },
    body: status === 204 ? '' : JSON.stringify(body)
  });
}

async function sse(route: Route, events: unknown[], eventName = 'transcript') {
  await route.fulfill({
    status: 200,
    contentType: 'text/event-stream',
    headers: { 'X-Protocol-Version': '1.0' },
    body: events.map((event) => `event: ${eventName}\ndata: ${JSON.stringify(event)}\n\n`).join('')
  });
}
