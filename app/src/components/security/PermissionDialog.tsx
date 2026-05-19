import { useEffect, useState } from 'react';

export type Permission = 'file_read' | 'file_write' | 'network' | 'process' | 'system_info';

interface PermissionDialogProps {
  open: boolean;
  skillName: string;
  requested: Permission[];
  onGrant: (granted: Permission[]) => void;
  onDeny: () => void;
}

const LABEL: Record<Permission, string> = {
  file_read: '파일 읽기',
  file_write: '파일 쓰기',
  network: '네트워크 접근',
  process: '외부 프로세스 실행',
  system_info: '시스템 정보 조회',
};

const DESCRIPTION: Record<Permission, string> = {
  file_read: '워크스페이스의 파일을 읽을 수 있게 허용합니다.',
  file_write: '워크스페이스의 파일을 생성·수정할 수 있게 허용합니다.',
  network: '외부 네트워크에 요청을 보낼 수 있게 허용합니다.',
  process: '시스템 명령을 실행할 수 있게 허용합니다.',
  system_info: 'OS / 하드웨어 정보를 조회할 수 있게 허용합니다.',
};

/**
 * Presentation-only permission grant dialog (PRD §3.4 V4-7).
 *
 * The deny-all default is enforced by `security::permissions::PermissionGate`
 * on the Rust side. This component asks the user explicitly for each
 * requested category and emits the *selected* subset on grant; deny short-
 * circuits to an empty grant list via `onDeny`.
 */
export default function PermissionDialog({
  open,
  skillName,
  requested,
  onGrant,
  onDeny,
}: PermissionDialogProps) {
  const [selected, setSelected] = useState<Set<Permission>>(() => new Set(requested));

  // Reset selection whenever the requested set changes or the dialog reopens.
  useEffect(() => {
    if (open) {
      setSelected(new Set(requested));
    }
  }, [open, requested]);

  if (!open) return null;

  const toggle = (permission: Permission) => {
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(permission)) {
        next.delete(permission);
      } else {
        next.add(permission);
      }
      return next;
    });
  };

  const handleGrant = () => {
    onGrant(Array.from(selected));
  };

  return (
    <div
      data-testid="permission-dialog"
      role="dialog"
      aria-modal="true"
      aria-label={`${skillName} 권한 요청`}
      className="fixed inset-0 z-50 flex items-center justify-center bg-stone-900/60 p-4">
      <div className="w-full max-w-md rounded-lg bg-white p-5 shadow-xl">
        <h2 className="mb-1 text-lg font-semibold text-stone-900">권한 요청</h2>
        <p className="mb-4 text-sm text-stone-600">
          스킬 <strong data-testid="permission-dialog-skill">{skillName}</strong> 가 다음 권한을
          요청합니다.
        </p>

        <ul className="mb-4 space-y-2" data-testid="permission-dialog-list">
          {requested.map(permission => (
            <li key={permission} className="flex items-start gap-2">
              <input
                id={`perm-${permission}`}
                type="checkbox"
                checked={selected.has(permission)}
                onChange={() => toggle(permission)}
                aria-label={LABEL[permission]}
                data-testid={`permission-dialog-check-${permission}`}
                className="mt-1"
              />
              <label htmlFor={`perm-${permission}`} className="text-sm">
                <span className="font-medium text-stone-800">{LABEL[permission]}</span>
                <span className="ml-1 text-stone-500">— {DESCRIPTION[permission]}</span>
              </label>
            </li>
          ))}
        </ul>

        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onDeny}
            data-testid="permission-dialog-deny"
            className="rounded-md border border-stone-300 px-3 py-1 text-sm text-stone-700 hover:bg-stone-50">
            거부
          </button>
          <button
            type="button"
            onClick={handleGrant}
            data-testid="permission-dialog-grant"
            className="rounded-md bg-ocean-600 px-3 py-1 text-sm font-medium text-white hover:bg-ocean-700">
            허용 ({selected.size})
          </button>
        </div>
      </div>
    </div>
  );
}
