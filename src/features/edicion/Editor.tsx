import { AlertTriangle, ArrowDown, ArrowUp, Check, CircleMinus, Eye, Minus } from 'lucide-react';
import { useLayoutEffect, useRef } from 'react';
import { checkedDraft } from '../../store/preview';
import type { EditorScreen, NameDraft } from '../../store/preview';
import type { Mode, RowStatus, Rule, Sort, SortColumn } from '../../types';
import { NameEditor } from './NameEditor';
import { RulePanel } from './RulePanel';

type Props = {
  screen: EditorScreen;
  rule: Rule;
  sort: Sort;
  excludedIds: string[];
  manualNames: Record<string, string>;
  draft: NameDraft | null;
  advanced: boolean;
  onAdvanced(visible: boolean): void;
  onMode(mode: Mode): void;
  onEdit(rule: Rule): void;
  onSort(column: SortColumn): void;
  onToggleFile(id: string): void;
  onToggleAll(): void;
  onOpenName(id: string): void;
  onEditName(base: string): void;
  onSaveName(): void;
  onCancelName(): void;
  onRestoreName(id: string): void;
  onFocusSelect(): void;
  onRetry(): void;
};

const sizes = new Intl.NumberFormat('es', { maximumFractionDigits: 1 });
const statusLabels = {
  ready: { text: 'Listo', icon: Check },
  unchanged: { text: 'Sin cambios', icon: CircleMinus },
  excluded: { text: 'Excluido', icon: Minus },
  conflict: { text: 'Conflicto', icon: AlertTriangle },
  error: { text: 'Error', icon: AlertTriangle },
};
const columns: { key: SortColumn; label: string }[] = [
  { key: 'name', label: 'Nombre actual' },
  { key: 'type', label: 'Tipo' },
  { key: 'size', label: 'Tamaño' },
];

function formatSize(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const unit = bytes < 1024 ? 0 : Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return sizes.format(bytes / 1024 ** unit) + ' ' + units[unit];
}

function StatusCell({ status, reason }: { status: RowStatus | null; reason?: string | null }) {
  if (!status) return <span className="muted">—</span>;
  const { text, icon: Icon } = statusLabels[status];
  return <span className={'row-status ' + status}><Icon size={16} aria-hidden="true" /><span>{text}{reason && <small>{reason}</small>}</span></span>;
}

export function Editor({
  screen, rule, sort, excludedIds, manualNames, draft, advanced, onAdvanced, onMode,
  onEdit, onSort, onToggleFile, onToggleAll, onOpenName, onEditName, onSaveName,
  onCancelName, onRestoreName, onFocusSelect, onRetry,
}: Props) {
  const { selection, orderedIds, view } = screen;
  const rows = 'rows' in view ? view.rows : [];
  const byId = new Map(rows.map((row) => [row.fileId, row]));
  const files = new Map(selection.files.map((file) => [file.id, file]));
  const excluded = new Set(excludedIds);
  const includeAll = useRef<HTMLInputElement>(null);
  const table = useRef<HTMLTableSectionElement>(null);
  const previousDraft = useRef<{ id: string; selection: number } | null>(null);
  const mixed = excluded.size > 0 && excluded.size < selection.files.length;
  useLayoutEffect(() => {
    if (includeAll.current) includeAll.current.indeterminate = mixed;
  }, [mixed]);
  useLayoutEffect(() => {
    if (draft) {
      previousDraft.current = { id: draft.fileId, selection: selection.revision };
      return;
    }
    const previous = previousDraft.current;
    if (!previous) return;
    previousDraft.current = null;
    const row = previous.selection === selection.revision
      ? table.current?.querySelector('[data-file-id="' + CSS.escape(previous.id) + '"]')
      : null;
    const control = row?.querySelector<HTMLElement>('[data-edit-name]') ??
      row?.querySelector<HTMLElement>('input[type="checkbox"]') ?? includeAll.current;
    if (control) control.focus();
    else onFocusSelect();
  }, [draft, selection.revision, onFocusSelect]);
  const errors = view.status === 'invalid' ? view.errors : [];
  const needsDistinctNames = rows.some(({ reason }) => reason === 'No se pudo verificar que los destinos propuestos sean distintos.');
  const count = (status: RowStatus) => rows.filter((row) => row.status === status).length;
  const summary = view.status === 'loading' ? 'Calculando vista previa…' :
    view.status === 'invalid' ? 'Corrige la regla para ver la vista previa' :
    view.status === 'failed' ? view.message :
    view.status === 'folderFailure' ? view.reason :
    [
      count('ready') + ' listos',
      count('unchanged') + ' sin cambios',
      count('excluded') + ' excluidos',
      count('conflict') + ' con conflicto',
      count('error') + ' con error',
    ].join(' · ');
  const Arrow = sort.direction === 'asc' ? ArrowUp : ArrowDown;
  const retry = view.status === 'failed' ||
    (view.status === 'folderFailure' && view.action === 'retry') ||
    (view.status === 'ready' && rows.some((row) => row.retryable && !excluded.has(row.fileId)));
  const draftRow = draft ? checkedDraft(screen, draft) : undefined;
  const editorRow = draft ? byId.get(draft.fileId) : undefined;
  const draftMessage = view.status === 'failed' ? view.message : view.status === 'folderFailure' ? view.reason :
    draftRow?.nameEdit?.saveReason ?? editorRow?.reason ?? null;

  return (
    <>
      <div className="editor">
        <section className="panel preview" aria-labelledby="preview-title">
          <header className="panel-heading">
            <h1 id="preview-title"><Eye size={22} aria-hidden="true" />Vista previa</h1>
            <p>Los archivos permanecen intactos</p>
          </header>
          <div className="table-scroll" tabIndex={0} role="region" aria-label="Archivos seleccionados y nombres propuestos">
            <table>
              <thead>
                <tr>
                  <th scope="col" className="check-cell">
                    <label className="include-all">
                      <input type="checkbox" ref={includeAll} checked={excluded.size === 0 && selection.files.length > 0}
                        aria-label="Incluir todos los archivos" aria-checked={mixed ? 'mixed' : undefined}
                        disabled={selection.files.length === 0} onChange={onToggleAll} />
                      Incluir
                    </label>
                  </th>
                  {columns.map(({ key, label }) => (
                    <th key={key} scope="col" aria-sort={sort.column === key ? (sort.direction === 'asc' ? 'ascending' : 'descending') : undefined}>
                      <button className="sort-button" onClick={() => onSort(key)}>
                        {label}{sort.column === key && <Arrow size={13} aria-hidden="true" />}
                      </button>
                    </th>
                  ))}
                  <th scope="col">Nombre propuesto</th>
                  <th scope="col">Estado</th>
                </tr>
              </thead>
              <tbody ref={table}>
                {orderedIds.map((id) => {
                  const file = files.get(id);
                  if (!file) return null;
                  const row = byId.get(id);
                  const include = !excluded.has(id);
                  return (
                    <tr key={selection.revision + ':' + id} data-file-id={id}>
                      <td className="check-cell">
                        <input type="checkbox" checked={include} aria-label={'Incluir ' + file.name} onChange={() => onToggleFile(id)} />
                      </td>
                      <td className="mono current">{file.name}</td>
                      <td className="muted">{file.kind}</td>
                      <td className="muted size">{formatSize(file.size)}</td>
                      <td>
                        {row?.proposedName ? <span className="candidate mono">{row.proposedName}</span> : <span className="muted">—</span>}
                        {include && (row?.nameEdit || Object.hasOwn(manualNames, id)) && (
                          <div className="name-row-actions">
                            {row?.nameEdit && <button type="button" data-edit-name onClick={() => onOpenName(id)}
                              aria-label={'Editar nombre de ' + file.name}>Editar nombre</button>}
                            {Object.hasOwn(manualNames, id) && <>
                              <span className="muted">Manual</span>
                              <button type="button" onClick={() => onRestoreName(id)} aria-label={'Restaurar nombre automático de ' + file.name}>
                                Restaurar automático
                              </button>
                            </>}
                          </div>
                        )}
                      </td>
                      <td><StatusCell status={include ? row?.status ?? null : 'excluded'} reason={row?.reason} /></td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
        <RulePanel rule={rule} errors={errors} advanced={advanced} onAdvanced={onAdvanced} onMode={onMode} onEdit={onEdit} />
      </div>
      <footer className="panel summary">
        <div>
          <span role="status">{summary}</span>
          {needsDistinctNames && (
            <p className="muted">{rule.mode === 'numbering'
              ? 'Añade una parte que distinga los nombres, como el contador {n}.'
              : 'Conserva una parte distinta del nombre actual en cada nombre propuesto.'}</p>
          )}
        </div>
        {!draft && retry && (
          <button className="select" onClick={onRetry}>Reintentar</button>
        )}
      </footer>
      {draft && <NameEditor draft={draft} fileName={files.get(draft.fileId)?.name ?? ''}
        message={draftMessage} pending={view.status === 'loading'} canSave={draftRow?.nameEdit?.canSave === true}
        canRetry={view.status === 'failed' || (view.status === 'folderFailure' && view.action === 'retry') || editorRow?.retryable === true}
        onEdit={onEditName} onSave={onSaveName} onCancel={onCancelName} onRetry={onRetry} />}
    </>
  );
}
