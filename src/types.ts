export type Mode = 'episode' | 'regex' | 'numbering' | 'text';

export type Extension = { kind: 'keep' | 'remove' } | { kind: 'change'; value: string };

export type Rule =
  | { mode: 'episode'; season: string; digits: string; pattern: string; template: string; extension: Extension }
  | { mode: 'regex'; pattern: string; replacement: string; ignoreCase: boolean; extension: Extension }
  | { mode: 'numbering'; start: string; step: string; width: string; template: string; extension: Extension }
  | { mode: 'text'; search: string; replacement: string; prefix: string; suffix: string; capitalization: 'keep' | 'upper' | 'lower'; extension: Extension };

export type FileEntry = { id: string; name: string; size: number; kind: string };
export type Selection = { revision: number; files: FileEntry[] };
export type PickResult =
  | { status: 'cancelled' }
  | { status: 'rejected'; reason: string }
  | { status: 'replaced'; selection: Selection };

export type SortColumn = 'name' | 'type' | 'size';
export type Sort = { column: SortColumn; direction: 'asc' | 'desc' };
export type Revision = { selectionRevision: number; editRevision: number };
export type ManualName = { fileId: string; base: string };
export type PreviewRequest = Revision & {
  config: Rule;
  sort: Sort;
  excludedIds: string[];
  manualNames?: ManualName[];
  draftFileId?: string;
};

export type RowStatus = 'ready' | 'unchanged' | 'excluded' | 'conflict' | 'error';
export type NameEdit = {
  base: string;
  extension: string;
  source: 'manual' | 'automatic' | 'noMatch' | 'generationError' | 'profileUnavailable';
  canSave: boolean;
  saveReason: string | null;
};
export type PreviewRow = {
  fileId: string;
  status: RowStatus | null;
  proposedName: string | null;
  reason: string | null;
  nameEdit: NameEdit | null;
  retryable: boolean;
};
export type Field = 'season' | 'digits' | 'pattern' | 'template' | 'start' | 'step' | 'width' | 'extension';
export type FieldError = { field: Field; message: string };
export type Preview =
  | { status: 'ready'; rows: PreviewRow[] }
  | { status: 'invalid'; rows: PreviewRow[]; errors: FieldError[] }
  | { status: 'folderFailure'; rows: PreviewRow[]; action: 'retry' | 'reselect'; reason: string }
  | { status: 'stale' };

export interface Api {
  pickFiles(): Promise<PickResult>;
  notifyRevision(request: Revision): Promise<void>;
  preview(request: PreviewRequest): Promise<Revision & Preview>;
}
