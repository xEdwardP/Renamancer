import { createStore } from 'zustand/vanilla';
import type { Api, Preview, Rule, Selection } from '../types.ts';

const defaultRule: Rule = {
  season: '1',
  digits: '2',
  pattern: '([0-9]+)[^0-9]*$',
  template: 'S{temporada}E{episodio}{ext}',
};

type View = { status: 'loading' } | { status: 'failed'; message: string } | Exclude<Preview, { status: 'stale' }>;

export type EditorScreen = { kind: 'edicion'; selection: Selection; view: View };
type Screen = { kind: 'inicio' } | EditorScreen;

type PreviewState = {
  screen: Screen;
  rule: Rule;
  picking: boolean;
  notice: string | null;
  advanced: boolean;
  clearNotice(): void;
  setAdvanced(advanced: boolean): void;
  pickFiles(): Promise<void>;
  edit(field: keyof Rule, value: string): void;
};

export function createPreviewStore(api: Api, debounce = 140) {
  return createStore<PreviewState>((set, get) => {
    let generation = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;

    function schedule(selection: Selection, rule: Rule) {
      const request = ++generation;
      clearTimeout(timer);
      set({ screen: { kind: 'edicion', selection, view: { status: 'loading' } }, rule });

      timer = setTimeout(async () => {
        try {
          const response = await api.preview(selection.revision, rule);
          if (request !== generation || response.revision !== selection.revision || response.preview.status === 'stale') {
            return;
          }
          const view = response.preview;
          const openAdvanced = view.status === 'invalid' && view.errors.some(({ field }) => field === 'pattern' || field === 'template');
          set({ screen: { kind: 'edicion', selection, view }, advanced: openAdvanced || get().advanced });
        } catch (error) {
          if (request === generation) {
            const message = typeof error === 'string' ? error : 'No se pudo calcular la vista previa.';
            set({ screen: { kind: 'edicion', selection, view: { status: 'failed', message } } });
          }
        }
      }, debounce);
    }

    return {
      screen: { kind: 'inicio' },
      rule: defaultRule,
      advanced: false,
      picking: false,
      notice: null,
      clearNotice: () => set({ notice: null }),
      setAdvanced: (advanced) => set({ advanced }),
      async pickFiles() {
        if (get().picking) return;
        set({ picking: true, notice: null });
        try {
          const result = await api.pickFiles();
          if (result.status === 'replaced') schedule(result.selection, get().rule);
          if (result.status === 'rejected') set({ notice: result.reason });
        } catch (error) {
          set({ notice: typeof error === 'string' ? error : 'No se pudo abrir el selector de archivos.' });
        } finally {
          set({ picking: false });
        }
      },
      edit(field, value) {
        const { screen, rule } = get();
        if (screen.kind === 'edicion' && rule[field] !== value) {
          schedule(screen.selection, { ...rule, [field]: value });
        }
      },
    };
  });
}
