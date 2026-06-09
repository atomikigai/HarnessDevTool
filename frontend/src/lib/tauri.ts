/**
 * Thin Tauri IPC wrapper with browser fallback.
 * In Tauri: calls native Rust commands (no WASM sandbox, full native speed).
 * In browser (dev / web build): falls back to the JS equivalent.
 */

export const isTauri: boolean = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
let _invoke: InvokeFn | null = null;

async function getInvoke(): Promise<InvokeFn> {
  if (_invoke) return _invoke;
  if (!isTauri) throw new Error('Not running in Tauri');
  const { invoke } = await import('@tauri-apps/api/core');
  _invoke = invoke as InvokeFn;
  return _invoke;
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>
): Promise<T> {
  const fn = await getInvoke();
  return fn(command, args) as Promise<T>;
}

export type NativePtyStreamEvent =
  | { type: 'started' }
  | { type: 'exit'; code?: number | null; signal?: string | null }
  | { type: 'lagged'; skipped: number; resync?: string | null }
  | { type: 'error'; message: string };

export interface NativePtyStreamHandle {
  close: () => void;
}

function bytesFromNativePayload(payload: unknown): Uint8Array {
  if (payload instanceof Uint8Array) return payload;
  if (payload instanceof ArrayBuffer) return new Uint8Array(payload);
  if (Array.isArray(payload)) return new Uint8Array(payload as number[]);
  if (
    payload &&
    typeof payload === 'object' &&
    'buffer' in payload &&
    (payload as { buffer?: unknown }).buffer instanceof ArrayBuffer
  ) {
    const view = payload as { buffer: ArrayBuffer; byteOffset?: number; byteLength?: number };
    return new Uint8Array(
      view.buffer,
      view.byteOffset ?? 0,
      view.byteLength ?? view.buffer.byteLength
    );
  }
  return new Uint8Array();
}

export async function streamPtyOutputNative(
  sessionId: string,
  onOutput: (bytes: Uint8Array) => void,
  onEvent: (event: NativePtyStreamEvent) => void
): Promise<NativePtyStreamHandle> {
  if (!isTauri) throw new Error('Not running in Tauri');
  const { Channel } = await import('@tauri-apps/api/core');
  let streamId: number | null = null;
  let closed = false;
  const output = new Channel<unknown>((payload) => {
    if (closed) return;
    const bytes = bytesFromNativePayload(payload);
    if (bytes.length > 0) onOutput(bytes);
  });
  const events = new Channel<NativePtyStreamEvent>((event) => {
    if (closed) return;
    onEvent(event);
  });

  streamId = await invokeCommand<number>('stream_pty_output', {
    sessionId,
    onOutput: output,
    onEvent: events
  });

  if (closed) {
    await invokeCommand<void>('stop_pty_output_stream', { streamId }).catch(() => {});
  }

  return {
    close: () => {
      closed = true;
      output.onmessage = () => {};
      events.onmessage = () => {};
      if (streamId !== null) {
        void invokeCommand<void>('stop_pty_output_stream', { streamId }).catch(() => {});
      }
    }
  };
}
