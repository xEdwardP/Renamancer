import { FilePlus2, ListRestart, X } from 'lucide-react';
import { useCallback, useRef } from 'react';
import { useStore } from 'zustand';
import { desktopApi } from './api/desktop';
import { Home } from './features/inicio/Home';
import { Editor } from './features/edicion/Editor';
import { createPreviewStore } from './store/preview';

type Store = ReturnType<typeof createPreviewStore>;
const desktopStore = createPreviewStore(desktopApi);

export function App({ store = desktopStore }: { store?: Store }) {
  const {
    screen, mode, forms, sort, excludedIds, manualNames, draft, picking, notice, advanced,
    pickFiles, clearNotice, edit, setMode, setAdvanced, sortBy, toggleFile, toggleAll, retry,
    openNameEditor, editName, saveName, cancelName, restoreName,
  } = useStore(store);
  const selectRef = useRef<HTMLButtonElement>(null);
  const focusSelect = useCallback(() => selectRef.current?.focus(), []);
  const count = screen.kind === 'edicion' ? screen.selection.files.length : 0;
  const select = (
    <button ref={selectRef} className={screen.kind === 'inicio' ? 'select primary' : 'select'} disabled={picking} onClick={pickFiles}>
      <FilePlus2 size={19} aria-hidden="true" />
      {picking ? 'Abriendo selector…' : 'Seleccionar archivos'}
    </button>
  );

  return (
    <main className="shell">
      <header className="brand"><span className="brand-icon"><ListRestart size={20} aria-hidden="true" /></span>Renamancer</header>
      {screen.kind === 'edicion' && (
        <div className="panel toolbar">{select}<p><strong>{count}</strong> {count === 1 ? 'archivo seleccionado' : 'archivos seleccionados'}</p></div>
      )}
      {notice && (
        <div className="notice" role="alert">
          {notice}<button aria-label="Cerrar aviso" onClick={clearNotice}><X size={18} aria-hidden="true" /></button>
        </div>
      )}
      {screen.kind === 'inicio' ? <Home select={select} /> : (
        <Editor screen={screen} rule={forms[mode]} sort={sort} excludedIds={excludedIds}
          manualNames={manualNames} draft={draft} advanced={advanced} onFocusSelect={focusSelect}
          onAdvanced={setAdvanced} onMode={setMode} onEdit={edit} onSort={sortBy}
          onToggleFile={toggleFile} onToggleAll={toggleAll} onOpenName={openNameEditor}
          onEditName={editName} onSaveName={saveName} onCancelName={cancelName}
          onRestoreName={restoreName} onRetry={retry} />
      )}
    </main>
  );
}
