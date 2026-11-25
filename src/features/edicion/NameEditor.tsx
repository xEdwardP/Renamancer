import { useLayoutEffect, useRef } from 'react';
import type { NameDraft } from '../../store/preview';

type Props = {
  draft: NameDraft;
  fileName: string;
  message: string | null;
  pending: boolean;
  canSave: boolean;
  canRetry: boolean;
  onEdit(base: string): void;
  onSave(): void;
  onCancel(): void;
  onRetry(): void;
};

export function NameEditor({ draft, fileName, message, pending, canSave, canRetry, onEdit, onSave, onCancel, onRetry }: Props) {
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);

  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    element.showModal();
    input.current?.focus();
    return () => element.close();
  }, []);

  return (
    <dialog ref={dialog} className="name-editor panel" aria-labelledby="name-editor-title"
      onCancel={(event) => { event.preventDefault(); onCancel(); }}>
      <form onSubmit={(event) => { event.preventDefault(); if (canSave) onSave(); }}>
        <h2 id="name-editor-title">Editar nombre</h2>
        <p className="muted mono">{fileName}</p>
        <div className="name-fields">
          <div className="field">
            <label htmlFor="manual-base">Nombre base</label>
            <input id="manual-base" ref={input} value={draft.base} spellCheck={false}
              aria-describedby="name-editor-result" onChange={(event) => onEdit(event.target.value)} />
          </div>
          <div className="field">
            <span>Extensión</span>
            <span className="mono name-extension">{draft.extension || 'Sin extensión'}</span>
          </div>
        </div>
        <p id="name-editor-result" className="name-result muted" role="status">{pending ? 'Comprobando nombre…' : message}</p>
        <div className="name-actions">
          {canRetry && <button type="button" className="select" onClick={onRetry}>Reintentar</button>}
          <button type="button" className="select" onClick={onCancel}>Cancelar</button>
          <button type="submit" className="select primary" disabled={!canSave}>Guardar</button>
        </div>
      </form>
    </dialog>
  );
}
