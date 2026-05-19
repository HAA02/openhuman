import { useCallback, useEffect, useState } from 'react';

import { callCoreRpc } from '../../services/coreRpcClient';

interface AuditRecord {
  timestamp?: string;
  kind?: string;
  channel?: string;
  user_id?: string;
  session_id?: string;
  status?: string;
  cost_usd?: number;
  model?: string;
  [key: string]: unknown;
}

interface AuditPayload {
  records: AuditRecord[];
  has_more: boolean;
  source: string;
}

interface AuditViewProps {
  /** Page size for each fetch. Defaults to 50, clamped server-side to 1..1000. */
  limit?: number;
}

function formatTimestamp(value: unknown): string {
  if (typeof value !== 'string' || value.length === 0) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString('ko-KR', { hour12: false });
}

/**
 * Read-only viewer for security audit JSONL records (PRD Phase 2).
 *
 * Fetches the most recent `limit` records, optionally filtered by a `since`
 * lower bound. The Rust core sorts append-order; this component does not
 * reorder.
 */
export default function AuditView({ limit = 50 }: AuditViewProps) {
  const [records, setRecords] = useState<AuditRecord[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [source, setSource] = useState<string | null>(null);
  const [since, setSince] = useState('');
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchAudit = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const payload = await callCoreRpc<AuditPayload>({
        method: 'security.get_audit',
        params: { since: since || undefined, limit },
      });
      setRecords(payload.records);
      setHasMore(payload.has_more);
      setSource(payload.source);
    } catch (e) {
      setError(e instanceof Error ? e.message : '감사 로그를 불러오지 못했습니다.');
    } finally {
      setLoading(false);
    }
  }, [since, limit]);

  useEffect(() => {
    void fetchAudit();
  }, [fetchAudit]);

  return (
    <section data-testid="audit-view" aria-label="감사 로그 뷰" className="space-y-3">
      <header className="flex flex-wrap items-end gap-3">
        <div className="flex flex-col">
          <label htmlFor="audit-since" className="text-xs text-stone-500">
            이 시각 이후 (RFC3339)
          </label>
          <input
            id="audit-since"
            type="text"
            placeholder="2026-05-19T00:00:00Z"
            value={since}
            onChange={event => setSince(event.target.value.trim())}
            className="w-64 rounded-md border border-stone-300 px-2 py-1 text-sm"
          />
        </div>
        <button
          type="button"
          onClick={() => void fetchAudit()}
          disabled={loading}
          data-testid="audit-view-refresh"
          className="rounded-md bg-ocean-600 px-3 py-1 text-sm font-medium text-white hover:bg-ocean-700 disabled:opacity-50"
        >
          {loading ? '불러오는 중…' : '새로 고침'}
        </button>
        {source && (
          <span data-testid="audit-view-source" className="text-xs text-stone-500">
            출처: {source}
          </span>
        )}
      </header>

      {error && (
        <p data-testid="audit-view-error" role="alert" className="text-sm text-coral-600">
          {error}
        </p>
      )}

      {!error && records.length === 0 && !loading && (
        <p data-testid="audit-view-empty" className="text-sm text-stone-500">
          표시할 감사 레코드가 없습니다.
        </p>
      )}

      {records.length > 0 && (
        <table className="w-full text-left text-sm" data-testid="audit-view-table">
          <thead className="text-xs uppercase text-stone-500">
            <tr>
              <th className="py-1 pr-3">시각</th>
              <th className="py-1 pr-3">종류</th>
              <th className="py-1 pr-3">상태</th>
              <th className="py-1 pr-3">채널</th>
              <th className="py-1 pr-3">비용</th>
              <th className="py-1">모델</th>
            </tr>
          </thead>
          <tbody>
            {records.map((record, idx) => {
              const ts = (record.timestamp as string | undefined) ?? '';
              const key = `${ts}-${idx}`;
              return (
                <tr
                  key={key}
                  data-testid="audit-view-row"
                  className="border-t border-stone-100 align-top"
                >
                  <td className="py-1 pr-3 font-mono text-xs">{formatTimestamp(record.timestamp)}</td>
                  <td className="py-1 pr-3">{record.kind ?? '—'}</td>
                  <td className="py-1 pr-3">{record.status ?? '—'}</td>
                  <td className="py-1 pr-3">{record.channel ?? '—'}</td>
                  <td className="py-1 pr-3">
                    {typeof record.cost_usd === 'number' ? `$${record.cost_usd.toFixed(4)}` : '—'}
                  </td>
                  <td className="py-1">{record.model ?? '—'}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}

      {hasMore && (
        <p data-testid="audit-view-has-more" className="text-xs text-stone-500">
          더 오래된 레코드가 더 있습니다. since 값을 좁혀서 다시 조회하세요.
        </p>
      )}
    </section>
  );
}
