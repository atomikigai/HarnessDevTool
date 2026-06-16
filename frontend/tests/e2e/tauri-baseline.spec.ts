import { expect, test, type Page, type Route } from '@playwright/test';

const session = {
  id: 's-chat-1',
  kind: 'codex',
  thread_id: 't-1',
  cwd: '/tmp/harness',
  pid: 1234,
  status: 'running',
  started_at: new Date().toISOString(),
  exit_code: null,
  role: 'generator',
  owner_session_id: null,
  task_id: 'TASK-1',
  scopes: [],
  repo: null,
  loaded_capabilities: { mcp_servers: [], skills: [], tool_groups: [] },
  parent_session_id: null,
  root_session_id: 's-chat-1',
  detected_state: 'idle',
  has_transcript: true
};

const readiness = {
  status: 'ready',
  checked_at: Date.now(),
  cwd: '/tmp/harness',
  blocking: [],
  warnings: [],
  facts: {},
  suggested_execution_mode: 'standard'
};

type MockTranscriptEvent = ReturnType<typeof transcript> | Record<string, unknown>;

async function installApiMocks(
  page: Page,
  opts: {
    transcriptReady?: boolean;
    transcriptEvents?: MockTranscriptEvent[];
    transcriptReplayOnce?: boolean;
    ptyOutputText?: string;
    approvals?: unknown[];
  } = {}
) {
  let checkpointRequested = false;
  let clearRequested = false;
  const sessionInputChunks: string[] = [];
  let transcriptRequests = 0;
  const transcriptReady = opts.transcriptReady ?? true;
  const approvals = opts.approvals ?? [];

  await page.route(
    (url) => url.pathname === '/api' || url.pathname.startsWith('/api/'),
    async (route) => {
      const url = new URL(route.request().url());
      const path = url.pathname.replace(/^\/api/, '');
      const method = route.request().method();

      if (path === '/health') return json(route, { version: 'test', uptime_s: 1 });
      if (path === '/approvals') return json(route, approvals);
      if (path.startsWith('/approvals/') && path.endsWith('/decide') && method === 'POST') {
        return json(route, null);
      }
      if (path === '/profiles/active')
        return json(route, { active: 'default', reload_required: false });
      if (path === '/threads') {
        return json(route, [
          {
            id: 't-1',
            title: 'Mock Thread',
            created_at: new Date().toISOString(),
            execution_mode: 'standard',
            autonomy_profile: 'assisted',
            repo: null,
            readiness,
            sessions: [session]
          }
        ]);
      }
      if (path === '/threads/t-1/tasks') return json(route, []);
      if (path === '/sessions/s-chat-1/children') return json(route, []);
      if (path === '/sessions/s-chat-1/metrics') {
        return json(route, {
          session_id: session.id,
          thread_id: session.thread_id,
          kind: session.kind,
          model: 'gpt-5.5',
          prompt_tokens: 120,
          output_tokens: 80,
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
      if (path === '/sessions/s-chat-1/context') {
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
          pressure: 0.37,
          context_tokens: 74000,
          max_context_tokens: 200000,
          model: 'gpt-5.5',
          checkpoint_preview: 'CONTEXT CHECKPOINT next_action: keep testing markdown',
          checkpoint_structured: { next_action: 'keep testing markdown' },
          indexed_events: 2
        });
      }
      if (path === '/sessions/s-chat-1/context/search') {
        return json(route, {
          query: url.searchParams.get('q') ?? '',
          hits: [
            {
              thread_id: 't-1',
              session_id: 's-chat-1',
              event_type: 'session.context.checkpoint_saved',
              at: Date.now(),
              pressure: 0.37,
              model: 'gpt-5.5',
              snippet: 'next_action: keep testing markdown'
            }
          ]
        });
      }
      if (path === '/sessions/s-chat-1/context/checkpoint' && method === 'POST') {
        checkpointRequested = true;
        return json(route, { status: 'requested', reason: null });
      }
      if (path === '/sessions/s-chat-1/context/clear' && method === 'POST') {
        clearRequested = true;
        return json(route, { status: 'cleared', reason: null });
      }
      if (path === '/sessions/s-chat-1/transcript') {
        transcriptRequests += 1;
        if (!transcriptReady) return sse(route, [], 'transcript');
        if (opts.transcriptReplayOnce && transcriptRequests > 1) {
          return sse(route, [], 'transcript');
        }
        return sse(
          route,
          opts.transcriptEvents ?? [
            transcript({
              seq: 1,
              role: 'user',
              content: 'show markdown'
            }),
            transcript({
              seq: 2,
              role: 'assistant',
              content: '### Result\n\n- **bold** item\n\n```ts\nconst ok = true;\n```'
            })
          ]
        );
      }
      if (path === '/sessions/s-chat-1/input' && method === 'POST') {
        sessionInputChunks.push(await route.request().postData() ?? '');
        return json(route, null);
      }
      if (path.startsWith('/events')) {
        if (!transcriptReady) {
          const ptyOutputText =
            opts.ptyOutputText ?? '❯ hablame del equipo\n● Respuesta desde PTY fallback\n';
          return sse(
            route,
            [
              {
                type: 'session.output',
                session_id: 's-chat-1',
                seq: 1,
                b64: Buffer.from(ptyOutputText, 'utf8').toString('base64')
              }
            ],
            'session.output'
          );
        }
        return sse(route, [], 'session.output');
      }

      return json(route, {});
    }
  );

  return {
    checkpointRequested: () => checkpointRequested,
    clearRequested: () => clearRequested,
    sessionInputText: () => sessionInputChunks.join('')
  };
}

test('ChatView renders markdown transcript instead of waiting forever', async ({ page }) => {
  await installApiMocks(page);
  await page.goto('/');
  await openChat(page);

  await expect(page.getByText('Waiting for transcript events')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Result' })).toBeVisible();
  await expect(page.getByText('bold')).toBeVisible();
  await expect(page.locator('code').filter({ hasText: 'const ok = true' })).toBeVisible();
});

test('ChatView falls back to PTY output when transcript is not ready', async ({ page }) => {
  await installApiMocks(page, { transcriptReady: false });
  await page.goto('/');
  await openChat(page);

  await expect(page.getByText('Respuesta desde PTY fallback')).toBeVisible();
  await expect(page.getByText('Waiting for transcript events')).toHaveCount(0);
});

test('ChatView normalizes Claude-style PTY output when transcript is missing', async ({ page }) => {
  await installApiMocks(page, {
    transcriptReady: false,
    ptyOutputText: [
      'Human: summarize the repo',
      '● Reading package files',
      '⎿  Found 4 manifests',
      '✓ Claude-style response from PTY fallback'
    ].join('\n')
  });
  await page.goto('/');
  await openChat(page);

  await expect(page.getByText('Claude-style response from PTY fallback')).toBeVisible();
  await expect(page.locator('.pty-line-action').filter({ hasText: 'Reading package files' })).toBeVisible();
  await expect(page.locator('.pty-line-result').filter({ hasText: 'Claude-style response' })).toBeVisible();
});

test('ChatView keeps transcript when switching between chat and terminal', async ({ page }) => {
  await installApiMocks(page, { transcriptReplayOnce: true });
  await page.goto('/');
  await openChat(page);

  await expect(page.getByRole('heading', { name: 'Result' })).toBeVisible();
  await page.getByRole('button', { name: 'Terminal', exact: true }).click({ force: true });
  await expect(page.getByRole('button', { name: /Active codex/ })).toBeVisible();
  await page.getByRole('button', { name: 'Chat', exact: true }).click({ force: true });

  await expect(page.getByRole('heading', { name: 'Result' })).toBeVisible();
  await expect(page.getByText('Waiting for transcript events')).toHaveCount(0);
});

test('ChatView renders Claude PR link events as useful links', async ({ page }) => {
  await installApiMocks(page, {
    transcriptEvents: [
      transcript({ seq: 1, role: 'user', content: 'open a pr' }),
      {
        seq: 2,
        session_id: 's-chat-1',
        ts: new Date().toISOString(),
        source: 'claude',
        kind: 'unknown',
        role: null,
        content: null,
        tool_name: null,
        tool_args: null,
        tool_use_id: null,
        tool_result: null,
        is_error: null,
        model: null,
        usage: null,
        subtype: null,
        raw: {
          type: 'pr-link',
          url: 'https://github.com/acme/repo/pull/670',
          number: 670,
          title: 'fix: cron scheduler improvements'
        }
      }
    ]
  });
  await page.goto('/');
  await openChat(page);

  const link = page.getByRole('link', { name: /Pull request #670/ });
  await expect(link).toBeVisible();
  await expect(link).toHaveAttribute('href', 'https://github.com/acme/repo/pull/670');
  await expect(page.getByText('Unparsed claude event: pr-link')).toHaveCount(0);
});

test('ChatView exposes CLI approval choices inside the transcript', async ({ page }) => {
  const calls = await installApiMocks(page, {
    transcriptEvents: [
      transcript({ seq: 1, role: 'user', content: 'stage files' }),
      {
        seq: 2,
        session_id: 's-chat-1',
        ts: new Date().toISOString(),
        source: 'claude',
        kind: 'unknown',
        role: null,
        content: null,
        tool_name: null,
        tool_args: null,
        tool_use_id: null,
        tool_result: null,
        is_error: null,
        model: null,
        usage: null,
        subtype: null,
        raw: {
          type: 'approval-request',
          tool: 'Bash',
          command: 'git add .'
        }
      }
    ]
  });
  await page.goto('/');
  await openChat(page);

  await expect(page.getByText('Approval required: Bash')).toBeVisible();
  await page.locator('.system-approval').getByRole('button', { name: 'Always' }).click();
  await expect.poll(() => calls.sessionInputText()).toBe('2\r');
});

test('Context panel shows pressure, search and manual actions', async ({ page }) => {
  const calls = await installApiMocks(page);
  await page.goto('/');
  await expect(page.getByRole('button', { name: /^Info$/ })).toBeVisible({ timeout: 15000 });
  await page.getByRole('button', { name: /^Info$/ }).click({ force: true });

  await expect(page.getByText('37%')).toBeVisible();
  await page.getByPlaceholder('Search checkpoints').fill('markdown');
  await page.getByRole('button', { name: 'Search' }).click();
  await expect(page.getByText('next_action: keep testing markdown', { exact: true })).toBeVisible();

  await page.getByRole('button', { name: 'Checkpoint' }).click();
  await expect.poll(() => calls.checkpointRequested()).toBe(true);
  await page.getByRole('button', { name: 'Clear' }).click();
  await expect.poll(() => calls.clearRequested()).toBe(true);
});

test('Terminal tab mounts without unreadable empty-state regression', async ({ page }) => {
  await installApiMocks(page);
  await page.goto('/');
  const terminalTab = page.getByRole('button', { name: 'Terminal', exact: true });
  await expect(terminalTab).toBeVisible({ timeout: 15000 });
  await terminalTab.click({ force: true });

  await expect(page.getByRole('button', { name: /Active codex/ })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Terminal', exact: true })).toBeVisible();
});

async function openChat(page: Page) {
  const chatTab = page.getByRole('button', { name: 'Chat', exact: true });
  await expect(chatTab).toBeVisible({ timeout: 15000 });
  await chatTab.click({ force: true });
}

function transcript(input: { seq: number; role: 'user' | 'assistant'; content: string }) {
  return {
    seq: input.seq,
    session_id: 's-chat-1',
    ts: new Date().toISOString(),
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
    usage: null,
    subtype: null,
    raw: null
  };
}

async function json(route: Route, body: unknown) {
  await route.fulfill({
    status: 200,
    contentType: 'application/json',
    headers: { 'X-Protocol-Version': '1.0' },
    body: JSON.stringify(body)
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
