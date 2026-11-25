import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Api, NameEdit, PickResult, PreviewRequest, Revision, Rule } from '../types.ts';
import { createPreviewStore } from './preview.ts';

type Response = Awaited<ReturnType<Api['preview']>>;
type Request = PreviewRequest & {
  complete: (response: Response) => void;
  fail: (error: string) => void;
};

function fixture() {
  const selections: PickResult[] = [];
  const requests: Request[] = [];
  const notices: Revision[] = [];
  let nextRequest: ((request: Request) => void) | undefined;
  const store = createPreviewStore({
    async pickFiles() { return selections.shift() ?? { status: 'cancelled' }; },
    async notifyRevision(revision) { notices.push(revision); },
    preview(input) {
      return new Promise<Response>((complete, fail) => {
        const request = { ...input, complete, fail };
        if (nextRequest) {
          const deliver = nextRequest;
          nextRequest = undefined;
          deliver(request);
        } else {
          requests.push(request);
        }
      });
    },
  }, 0);

  return {
    store, selections, notices,
    async select(revision: number, ...names: string[]) {
      const files = names.map((name, index) => ({ id: 'f' + index, name, size: index + 20, kind: 'MP4' }));
      selections.push({ status: 'replaced', selection: { revision, files } });
      await store.getState().pickFiles();
    },
    next() {
      const request = requests.shift();
      return request ? Promise.resolve(request) : new Promise<Request>((resolve) => { nextRequest = resolve; });
    },
    screen() {
      const { screen } = store.getState();
      assert.equal(screen.kind, 'edicion');
      return screen;
    },
  };
}

function ready({ selectionRevision, editRevision }: Revision, name: string, nameEdit: NameEdit | null = null): Response {
  return {
    selectionRevision, editRevision, status: 'ready',
    rows: [{ fileId: 'f0', status: 'ready', proposedName: name, reason: null, nameEdit, retryable: false }],
  };
}

test('cancellation and rejection preserve the active batch and results', async () => {
  const app = fixture();
  await app.store.getState().pickFiles();
  assert.equal(app.store.getState().screen.kind, 'inicio');
  await app.select(1, 'video.1.mp4');
  const opened = await app.next();
  opened.complete(ready(opened, 'S01E01.mp4'));
  await Promise.resolve();
  const before = app.screen();

  await app.store.getState().pickFiles();
  assert.equal(app.screen(), before);
  app.selections.push({ status: 'rejected', reason: 'Carpetas distintas.' });
  await app.store.getState().pickFiles();
  assert.equal(app.screen(), before);
  assert.equal(app.store.getState().notice, 'Carpetas distintas.');
});

test('switching modes preserves edits and extensions, and a new batch clears exclusions only', async () => {
  const app = fixture();
  await app.select(1, 'video.1.mp4', 'video.2.mp4');
  const initial = await app.next();
  assert.equal(initial.config.mode, 'episode');
  const episode: Rule = { ...initial.config, season: '2', extension: { kind: 'change', value: 'mkv' } };
  app.store.getState().edit(episode);
  app.store.getState().sortBy('size');
  app.store.getState().toggleFile('f0');
  app.store.getState().setMode('text');
  const text = await app.next();
  assert.deepEqual(text.config, {
    mode: 'text', search: '', replacement: '', prefix: '', suffix: '', capitalization: 'keep', extension: { kind: 'keep' },
  });
  assert.equal(text.config.mode, 'text');
  app.store.getState().edit({ ...text.config, prefix: 'Nuevo', extension: { kind: 'remove' } });
  app.store.getState().setMode('regex');
  const regex = await app.next();
  assert.deepEqual(regex.config, { mode: 'regex', pattern: '', replacement: '', ignoreCase: false, extension: { kind: 'keep' } });
  app.store.getState().setMode('episode');
  const back = await app.next();
  assert.deepEqual(back.config, episode);
  assert.deepEqual(back.excludedIds, ['f0']);
  assert.equal(back.sort.column, 'size');
  app.store.getState().setMode('text');
  const previous = await app.next();
  assert.equal(previous.config.mode, 'text');
  assert.equal(previous.config.prefix, 'Nuevo');
  assert.deepEqual(previous.config.extension, { kind: 'remove' });

  await app.select(2, 'video.9.mp4');
  const current = await app.next();
  assert.equal(current.selectionRevision, 2);
  assert.deepEqual(current.config, previous.config);
  assert.deepEqual(current.sort, previous.sort);
  assert.deepEqual(current.excludedIds, []);
  const result = ready(current, 'Nuevovideo.9');
  current.complete(result);
  previous.complete(ready(previous, 'Nuevovideo.1'));
  initial.fail('Error anterior');
  await Promise.resolve();
  assert.deepEqual(app.screen().view, result);
});

test('invalidation sends an immediate notice, clears old results and keeps order and checkboxes', async () => {
  const app = fixture();
  await app.select(1, 'video.1.mp4', 'video.2.mp4');
  const initial = await app.next();
  const result = ready(initial, 'S01E02.mp4');
  assert.equal(result.status, 'ready');
  initial.complete({ ...result, rows: [
    { fileId: 'f1', status: 'ready', proposedName: 'S01E02.mp4', reason: null, nameEdit: null, retryable: false },
    { fileId: 'f0', status: 'ready', proposedName: 'S01E01.mp4', reason: null, nameEdit: null, retryable: false },
  ] });
  await Promise.resolve();
  assert.deepEqual(app.screen().orderedIds, ['f1', 'f0']);

  app.store.getState().toggleFile('f1');
  assert.equal(app.screen().view.status, 'loading');
  assert.deepEqual(app.screen().orderedIds, ['f1', 'f0']);
  const obsolete = await app.next();
  app.store.getState().sortBy('size');
  app.store.getState().setMode('regex');
  assert.deepEqual(app.notices.at(-1), { selectionRevision: 1, editRevision: 4 });
  obsolete.complete(ready(obsolete, 'Antiguo.mp4'));
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'loading');

  const invalid = await app.next();
  invalid.complete({
    selectionRevision: 1, editRevision: invalid.editRevision, status: 'invalid',
    errors: [{ field: 'pattern', message: 'Introduce un patrón.' }],
    rows: [
      { fileId: 'f0', status: null, proposedName: null, reason: null, nameEdit: null, retryable: false },
      { fileId: 'f1', status: 'excluded', proposedName: null, reason: null, nameEdit: null, retryable: false },
    ],
  });
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'invalid');
  assert.deepEqual(app.store.getState().excludedIds, ['f1']);
  app.store.getState().toggleFile('f1');
  assert.deepEqual((await app.next()).excludedIds, []);
});

test('retry uses the current settings and discards late errors', async () => {
  const app = fixture();
  await app.select(1, 'video.1.mp4');
  const older = await app.next();
  app.store.getState().sortBy('type');
  const current = await app.next();
  current.complete({
    selectionRevision: 1, editRevision: current.editRevision, status: 'folderFailure',
    rows: [], action: 'retry', reason: 'No se pudo leer la carpeta.',
  });
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'folderFailure');
  app.store.getState().retry();
  const retry = await app.next();
  assert.equal(retry.editRevision, current.editRevision + 1);
  assert.deepEqual(retry.config, current.config);
  assert.deepEqual(retry.sort, current.sort);
  const result = ready(retry, 'S01E01.mp4');
  retry.complete(result);
  older.fail('Error anterior');
  await Promise.resolve();
  assert.deepEqual(app.screen().view, result);
});

test('bulk selection emits one revision per action and retains the displayed order', async () => {
  const app = fixture();
  const state = app.store.getState;
  await app.select(1, 'c.mp4', 'b.mp4', 'a.mp4');
  await app.next();
  state().toggleFile('f1');
  const partial = await app.next();
  state().toggleAll();
  assert.deepEqual(app.screen().orderedIds, ['f0', 'f1', 'f2']);
  const included = await app.next();
  state().toggleAll();
  const excluded = await app.next();
  assert.deepEqual(included.excludedIds, []);
  assert.deepEqual(excluded.excludedIds, ['f0', 'f1', 'f2']);
  assert.deepEqual(app.notices.slice(-3).map(({ editRevision }) => editRevision),
    [partial.editRevision, partial.editRevision + 1, partial.editRevision + 2]);
});

const automatic: NameEdit = {
  base: 'S01E01', extension: '.mp4', source: 'automatic', canSave: false, saveReason: null,
};

test('saving requires the current draft and corrections survive rule changes until restored', async () => {
  const app = fixture();
  const state = app.store.getState;
  await app.select(1, 'video.1.mp4');
  const initial = await app.next();
  initial.complete(ready(initial, 'S01E01.mp4', automatic));
  await Promise.resolve();
  state().openNameEditor('f0');
  state().editName('primero');
  const obsolete = await app.next();
  state().editName('revisado.v2');
  const current = await app.next();
  assert.equal(current.draftFileId, 'f0');
  assert.deepEqual(current.manualNames, [{ fileId: 'f0', base: 'revisado.v2' }]);
  obsolete.complete(ready(obsolete, 'primero.mp4', { ...automatic, canSave: true }));
  await Promise.resolve();
  state().saveName();
  assert.equal(state().draft?.base, 'revisado.v2');
  assert.deepEqual(state().manualNames, {});
  const result = ready(current, 'revisado.v2.mp4', {
    ...automatic, base: 'revisado.v2', source: 'manual', canSave: true,
  });
  current.complete(result);
  await Promise.resolve();
  state().saveName();
  assert.equal(state().draft, null);
  assert.equal(app.screen().view, result);
  assert.equal(app.notices.at(-1)?.editRevision, current.editRevision);

  state().toggleFile('f0');
  assert.deepEqual((await app.next()).manualNames, current.manualNames);
  state().toggleFile('f0');
  state().setMode('regex');
  state().sortBy('size');
  assert.deepEqual((await app.next()).manualNames, current.manualNames);
  state().restoreName('f0');
  assert.deepEqual((await app.next()).manualNames, []);
  assert.deepEqual(state().manualNames, {});
});

test('fallback retries keep the draft and cancellation supersedes pending and in-flight edits', async () => {
  const app = fixture();
  const state = app.store.getState;
  await app.select(1, 'video.mp4');
  const initial = await app.next();
  const fallback: NameEdit = { ...automatic, base: 'video', extension: '.mkv', source: 'generationError' };
  initial.complete(ready(initial, 'video.mkv', fallback));
  await Promise.resolve();
  state().openNameEditor('f0');
  const first = await app.next();
  assert.deepEqual(first.manualNames, [{ fileId: 'f0', base: 'video' }]);
  first.fail('Lectura interrumpida.');
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'failed');
  state().retry();
  const retry = await app.next();
  assert.deepEqual(retry.manualNames, first.manualNames);
  assert.equal(retry.draftFileId, 'f0');

  state().editName('cambio durante la pausa');
  state().cancelName();
  const cancelled = await app.next();
  assert.equal(cancelled.editRevision, retry.editRevision + 2);
  assert.deepEqual(cancelled.manualNames, []);
  assert.equal(cancelled.draftFileId, undefined);
  const result = ready(cancelled, 'video.mkv', fallback);
  cancelled.complete(result);
  retry.complete(ready(retry, 'video.mkv', { ...fallback, canSave: true }));
  await Promise.resolve();
  assert.equal(app.screen().view, result);

  state().openNameEditor('f0');
  const reopened = await app.next();
  reopened.complete(ready(reopened, 'video.mkv', { ...fallback, source: 'manual', canSave: true }));
  await Promise.resolve();
  state().saveName();
  assert.deepEqual(state().manualNames, { f0: 'video' });
  await app.select(2, 'otro.mp4');
  assert.deepEqual((await app.next()).manualNames, []);
});
