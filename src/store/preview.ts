import { createStore } from 'zustand/vanilla';
import type { Api, Mode, Preview, PreviewRow, Revision, Rule, Selection, Sort, SortColumn } from '../types.ts';

const defaultForms: Record<Mode, Rule> = {
  episode: { mode: 'episode', season: '1', digits: '2', pattern: '([0-9]+)[^0-9]*$', template: 'S{temporada}E{episodio}{ext}', extension: { kind: 'keep' } },
  regex: { mode: 'regex', pattern: '', replacement: '', ignoreCase: false, extension: { kind: 'keep' } },
  numbering: { mode: 'numbering', start: '1', step: '1', width: '2', template: '{n}', extension: { kind: 'keep' } },
  text: { mode: 'text', search: '', replacement: '', prefix: '', suffix: '', capitalization: 'keep', extension: { kind: 'keep' } },
};

type View = { status: 'loading' } | { status: 'failed'; message: string } | (Revision & Exclude<Preview, { status: 'stale' }>);

export type EditorScreen = { kind: 'edicion'; selection: Selection; orderedIds: string[]; view: View };
type Screen = { kind: 'inicio' } | EditorScreen;

export type NameDraft = {
  fileId: string;
  base: string;
  initialBase: string;
  extension: string;
  fallback: boolean;
  revision: Revision | null;
};

function usesDraft(draft: NameDraft) {
  return draft.fallback || draft.base !== draft.initialBase;
}

export function checkedDraft(screen: EditorScreen, draft: NameDraft): PreviewRow | undefined {
  const { view } = screen;
  return draft.revision && 'rows' in view &&
    draft.revision.selectionRevision === view.selectionRevision &&
    draft.revision.editRevision === view.editRevision
    ? view.rows.find(({ fileId }) => fileId === draft.fileId)
    : undefined;
}

type PreviewState = {
  screen: Screen;
  mode: Mode;
  forms: Record<Mode, Rule>;
  sort: Sort;
  excludedIds: string[];
  manualNames: Record<string, string>;
  draft: NameDraft | null;
  picking: boolean;
  notice: string | null;
  advanced: boolean;
  clearNotice(): void;
  setAdvanced(advanced: boolean): void;
  pickFiles(): Promise<void>;
  setMode(mode: Mode): void;
  edit(rule: Rule): void;
  sortBy(column: SortColumn): void;
  toggleFile(id: string): void;
  toggleAll(): void;
  openNameEditor(id: string): void;
  editName(base: string): void;
  saveName(): void;
  cancelName(): void;
  restoreName(id: string): void;
  retry(): void;
};

export function createPreviewStore(api: Api, debounce = 140) {
  return createStore<PreviewState>((set, get) => {
    let editRevision = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;

    function schedule(selection: Selection) {
      const { screen, mode, forms, sort, excludedIds, manualNames, draft } = get();
      const revision = { selectionRevision: selection.revision, editRevision: ++editRevision };
      const activeDraft = draft && usesDraft(draft) ? draft : null;
      const names = activeDraft ? { ...manualNames, [activeDraft.fileId]: activeDraft.base } : manualNames;
      const corrections = Object.entries(names).map(([fileId, base]) => ({ fileId, base }));
      const orderedIds = screen.kind === 'edicion' && screen.selection.revision === selection.revision
        ? screen.orderedIds
        : selection.files.map(({ id }) => id);
      const current = () => {
        const active = get().screen;
        return active.kind === 'edicion' && active.selection.revision === selection.revision && editRevision === revision.editRevision;
      };

      clearTimeout(timer);
      set({
        screen: { kind: 'edicion', selection, orderedIds, view: { status: 'loading' } },
        draft: draft ? { ...draft, revision } : null,
      });
      // The request also advances the revision if IPC delivers it before the notice.
      void api.notifyRevision(revision).catch(() => {});

      timer = setTimeout(async () => {
        try {
          const response = await api.preview({
            ...revision, config: forms[mode], sort, excludedIds,
            manualNames: corrections,
            draftFileId: activeDraft?.fileId,
          });
          if (!current() || response.selectionRevision !== revision.selectionRevision ||
              response.editRevision !== revision.editRevision || response.status === 'stale') return;
          const openAdvanced = mode === 'episode' && response.status === 'invalid' &&
            response.errors.some(({ field }) => field === 'pattern' || field === 'template');
          set({
            screen: { kind: 'edicion', selection, orderedIds: response.rows.map(({ fileId }) => fileId), view: response },
            advanced: openAdvanced || get().advanced,
          });
        } catch (error) {
          if (current()) {
            const message = typeof error === 'string' ? error : 'No se pudo calcular la vista previa.';
            set({ screen: { kind: 'edicion', selection, orderedIds, view: { status: 'failed', message } } });
          }
        }
      }, debounce);
    }

    function recalculate() {
      const { screen } = get();
      if (screen.kind === 'edicion') schedule(screen.selection);
    }

    return {
      screen: { kind: 'inicio' },
      mode: 'episode',
      forms: defaultForms,
      sort: { column: 'name', direction: 'asc' },
      excludedIds: [],
      manualNames: {},
      draft: null,
      advanced: false,
      picking: false,
      notice: null,
      clearNotice: () => set({ notice: null }),
      setAdvanced: (advanced) => set({ advanced }),
      async pickFiles() {
        if (get().picking || get().draft) return;
        set({ picking: true, notice: null });
        try {
          const result = await api.pickFiles();
          if (result.status === 'replaced') {
            editRevision = 0;
            set({ excludedIds: [], manualNames: {}, draft: null });
            schedule(result.selection);
          }
          if (result.status === 'rejected') set({ notice: result.reason });
        } catch (error) {
          set({ notice: typeof error === 'string' ? error : 'No se pudo abrir el selector de archivos.' });
        } finally {
          set({ picking: false });
        }
      },
      setMode(mode) {
        if (mode === get().mode || get().draft) return;
        set({ mode });
        recalculate();
      },
      edit(rule) {
        if (get().draft) return;
        set(({ forms }) => ({ forms: { ...forms, [rule.mode]: rule } }));
        recalculate();
      },
      sortBy(column) {
        if (get().draft) return;
        const { sort } = get();
        set({ sort: { column, direction: column === sort.column && sort.direction === 'asc' ? 'desc' : 'asc' } });
        recalculate();
      },
      toggleFile(id) {
        const { excludedIds, draft, screen } = get();
        if (draft || screen.kind !== 'edicion' || !screen.selection.files.some((file) => file.id === id)) return;
        set({ excludedIds: excludedIds.includes(id) ? excludedIds.filter((item) => item !== id) : [...excludedIds, id] });
        recalculate();
      },
      toggleAll() {
        const { draft, excludedIds, screen } = get();
        if (draft || screen.kind !== 'edicion' || screen.selection.files.length === 0) return;
        set({ excludedIds: excludedIds.length === 0 ? screen.selection.files.map(({ id }) => id) : [] });
        recalculate();
      },
      openNameEditor(id) {
        const { screen, draft, picking, excludedIds } = get();
        if (draft || picking || screen.kind !== 'edicion' || screen.view.status !== 'ready' || excludedIds.includes(id)) return;
        const info = screen.view.rows.find(({ fileId }) => fileId === id)?.nameEdit;
        if (!info) return;
        const fallback = info.source === 'generationError';
        set({ draft: { fileId: id, base: info.base, initialBase: info.base, extension: info.extension, fallback, revision: null } });
        if (fallback) recalculate();
      },
      editName(base) {
        const { draft } = get();
        if (!draft || draft.base === base) return;
        set({ draft: { ...draft, base, revision: null } });
        recalculate();
      },
      saveName() {
        const { screen, draft, manualNames } = get();
        if (!draft || !usesDraft(draft) || screen.kind !== 'edicion' || !checkedDraft(screen, draft)?.nameEdit?.canSave) return;
        set({ draft: null, manualNames: { ...manualNames, [draft.fileId]: draft.base } });
      },
      cancelName() {
        const { draft } = get();
        if (!draft) return;
        set({ draft: null });
        if (draft.revision) recalculate();
      },
      restoreName(id) {
        const { draft, manualNames } = get();
        if (draft || !Object.hasOwn(manualNames, id)) return;
        const next = { ...manualNames };
        delete next[id];
        set({ manualNames: next });
        recalculate();
      },
      retry: recalculate,
    };
  });
}
