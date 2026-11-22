export type Rule = {
  season: string;
  digits: string;
  pattern: string;
  template: string;
};

export type FileEntry = { name: string; size: number; kind: string };
export type Selection = { revision: number; files: FileEntry[] };
export type PickResult =
  | { status: 'cancelled' }
  | { status: 'rejected'; reason: string }
  | { status: 'replaced'; selection: Selection };

export type PreviewRow =
  | { status: 'proposed'; name: string }
  | { status: 'unchanged' }
  | { status: 'error'; reason: string };
export type FieldError = { field: keyof Rule; message: string };
export type Preview =
  | { status: 'ready'; rows: PreviewRow[] }
  | { status: 'invalid'; errors: FieldError[] }
  | { status: 'stale' };

export interface Api {
  pickFiles(): Promise<PickResult>;
  preview(revision: number, config: Rule): Promise<{ revision: number; preview: Preview }>;
}
