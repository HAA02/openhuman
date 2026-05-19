import { invoke as tauriInvoke } from '@tauri-apps/api/core';
import { useEffect, useRef, useState } from 'react';

import { callCoreRpc } from '../services/coreRpcClient';

interface Turn {
  id: string;
  role: 'user' | 'assistant';
  text: string;
  elapsedMs?: number;
  errored?: boolean;
}

interface RpcEnvelope<T> {
  result: T;
  logs: string[];
}

interface CodexStatus {
  available: boolean;
  version: string;
  error: string;
}

interface CodexComplete {
  content: string;
  model: string;
  elapsed_ms: number;
}

const CODEX_TIMEOUT_MS = 150_000;

export default function CodexChat() {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [input, setInput] = useState('');
  const [isSending, setIsSending] = useState(false);
  const [status, setStatus] = useState<CodexStatus | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    callCoreRpc<RpcEnvelope<CodexStatus>>({ method: 'openhuman.codex_status' })
      .then(envelope => {
        if (!cancelled) setStatus(envelope.result);
      })
      .catch(err => {
        if (!cancelled)
          setStatus({ available: false, version: '', error: String(err?.message ?? err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: 'smooth' });
  }, [turns]);

  const handleSend = async () => {
    const prompt = input.trim();
    if (!prompt || isSending) return;
    const userTurn: Turn = { id: `u-${Date.now()}`, role: 'user', text: prompt };
    setTurns(prev => [...prev, userTurn]);
    setInput('');
    setIsSending(true);
    try {
      // Route through the embedded core's JSON-RPC instead of Tauri's
      // direct invoke. Spawning codex from the Tauri main process
      // deadlocks on a futex inside codex (likely tokio runtime / signal
      // mask interaction with CEF). The embedded core is a separate
      // process and spawns codex cleanly.
      const envelope = await callCoreRpc<RpcEnvelope<CodexComplete>>({
        method: 'openhuman.codex_complete',
        params: { prompt, timeout_ms: CODEX_TIMEOUT_MS },
        timeoutMs: CODEX_TIMEOUT_MS + 10_000,
      });
      const result = envelope.result;
      void tauriInvoke;
      setTurns(prev => [
        ...prev,
        {
          id: `a-${Date.now()}`,
          role: 'assistant',
          text: result.content,
          elapsedMs: result.elapsed_ms,
        },
      ]);
    } catch (err) {
      setTurns(prev => [
        ...prev,
        {
          id: `e-${Date.now()}`,
          role: 'assistant',
          text: `Codex error: ${err instanceof Error ? err.message : String(err)}`,
          errored: true,
        },
      ]);
    } finally {
      setIsSending(false);
    }
  };

  const handleKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void handleSend();
    }
  };

  return (
    <div className="flex h-screen flex-col bg-stone-50">
      <header className="border-b border-stone-200 bg-white px-6 py-3">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-base font-semibold text-stone-900">Codex CLI (offline LLM)</h1>
            <p className="text-xs text-stone-500">
              Routes prompts through the local <code className="font-mono">codex</code> binary. No
              backend / API key required.
            </p>
          </div>
          <div className="text-right text-xs">
            {status === null ? (
              <span className="text-stone-400">Probing codex...</span>
            ) : status.available ? (
              <span className="rounded bg-emerald-50 px-2 py-0.5 font-medium text-emerald-700">
                {status.version}
              </span>
            ) : (
              <span className="rounded bg-red-50 px-2 py-0.5 font-medium text-red-700">
                unavailable
              </span>
            )}
          </div>
        </div>
      </header>

      <div ref={scrollRef} className="flex-1 space-y-4 overflow-y-auto px-6 py-6">
        {turns.length === 0 ? (
          <div className="mx-auto max-w-md rounded-lg border border-dashed border-stone-300 bg-white p-6 text-center text-sm text-stone-500">
            Send a prompt below. Cmd/Ctrl-Enter to submit.
            {status && !status.available ? (
              <div className="mt-3 rounded bg-red-50 p-2 text-left text-xs text-red-700">
                codex binary not found: <code>{status.error || 'PATH not set?'}</code>
              </div>
            ) : null}
          </div>
        ) : (
          turns.map(turn => (
            <div
              key={turn.id}
              className={`max-w-3xl rounded-lg px-4 py-3 text-sm shadow-soft ${
                turn.role === 'user'
                  ? 'ml-auto bg-primary-500 text-white'
                  : turn.errored
                    ? 'bg-red-50 text-red-800'
                    : 'bg-white text-stone-800'
              }`}>
              <div className="whitespace-pre-wrap font-sans leading-relaxed">{turn.text}</div>
              {turn.elapsedMs !== undefined ? (
                <div className="mt-1 text-[10px] uppercase tracking-wide text-stone-400">
                  {(turn.elapsedMs / 1000).toFixed(1)}s
                </div>
              ) : null}
            </div>
          ))
        )}
      </div>

      <footer className="border-t border-stone-200 bg-white px-6 py-4">
        <div className="flex items-end gap-3">
          <textarea
            value={input}
            onChange={e => setInput(e.target.value)}
            onKeyDown={handleKey}
            disabled={isSending}
            rows={3}
            placeholder="Type a prompt, Cmd/Ctrl-Enter to send..."
            className="min-w-0 flex-1 resize-none rounded-lg border border-stone-300 bg-white px-3 py-2 text-sm leading-relaxed focus:border-primary-500 focus:outline-none focus:ring-1 focus:ring-primary-500 disabled:opacity-50"
          />
          <button
            type="button"
            onClick={handleSend}
            disabled={!input.trim() || isSending}
            className="rounded-lg bg-primary-500 px-4 py-2 text-sm font-medium text-white shadow-soft transition-colors hover:bg-primary-600 disabled:cursor-not-allowed disabled:opacity-40">
            {isSending ? 'Thinking...' : 'Send'}
          </button>
        </div>
      </footer>
    </div>
  );
}
