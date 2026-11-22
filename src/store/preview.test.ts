import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { Api, PickResult, Rule } from '../types.ts';
import { createPreviewStore } from './preview.ts';

type Response = Awaited<ReturnType<Api['preview']>>;
type Request = {
  revision: number;
  config: Rule;
  complete: (response: Response) => void;
  fail: (error: string) => void;
};

function fixture() {
  const selections: PickResult[] = [];
  const requests: Request[] = [];
  let nextRequest: ((request: Request) => void) | undefined;
  const store = createPreviewStore({
    async pickFiles() {
      return selections.shift() ?? { status: 'cancelled' };
    },
    preview(revision, config) {
      return new Promise<Response>((complete, fail) => {
        const request = { revision, config, complete, fail };
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
    store,
    selections,
    async select(revision: number, name: string) {
      selections.push({ status: 'replaced', selection: { revision, files: [{ name, size: 20, kind: 'MP4' }] } });
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

function proposed(revision: number, name: string): Response {
  return { revision, preview: { status: 'ready', rows: [{ status: 'proposed', name }] } };
}

test('cancellation and rejection preserve the previous selection, rule and proposals', async () => {
  const app = fixture();
  await app.store.getState().pickFiles();
  assert.equal(app.store.getState().screen.kind, 'inicio');
  await app.select(1, 'video.1.mp4');
  const opened = await app.next();
  opened.complete(proposed(1, 'S01E01.mp4'));
  await Promise.resolve();
  const before = app.screen();
  const rule = app.store.getState().rule;

  await app.store.getState().pickFiles();
  assert.equal(app.screen(), before);
  app.selections.push({ status: 'rejected', reason: 'Carpetas distintas.' });
  await app.store.getState().pickFiles();
  assert.equal(app.screen(), before);
  assert.equal(app.store.getState().rule, rule);
  assert.equal(app.store.getState().notice, 'Carpetas distintas.');
});

test('a valid replacement retains the edited rule and discards an earlier batch response', async () => {
  const app = fixture();
  await app.select(1, 'video.1.mp4');
  const initial = await app.next();
  app.store.getState().edit('season', '2');
  const previous = await app.next();
  assert.equal(previous.config.season, '2');
  await app.select(2, 'video.9.mp4');
  assert.equal(app.screen().view.status, 'loading');
  const current = await app.next();
  assert.equal(current.revision, 2);
  assert.equal(current.config.season, '2');

  const result = proposed(2, 'S02E09.mp4');
  current.complete(result);
  previous.complete(proposed(1, 'S02E01.mp4'));
  initial.fail('Error anterior');
  await Promise.resolve();
  assert.equal(app.screen().selection.revision, 2);
  assert.deepEqual(app.screen().view, result.preview);
});

test('an invalid edit immediately clears proposals and discards an in-flight success and stale error', async () => {
  const app = fixture();
  await app.select(1, 'video.1.mp4');
  const initial = await app.next();
  initial.complete(proposed(1, 'S01E01.mp4'));
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'ready');

  app.store.getState().edit('season', '2');
  const pending = await app.next();
  app.store.getState().edit('pattern', '[');
  assert.equal(app.screen().view.status, 'loading');
  pending.complete(proposed(1, 'S02E01.mp4'));
  await Promise.resolve();
  assert.equal(app.screen().view.status, 'loading');

  const invalid = await app.next();
  assert.equal(invalid.config.pattern, '[');
  const response: Response = { revision: 1, preview: { status: 'invalid', errors: [{ field: 'pattern', message: 'Patrón no válido.' }] } };
  invalid.complete(response);
  await Promise.resolve();
  assert.deepEqual(app.screen().view, response.preview);
  assert.equal(app.store.getState().advanced, true);

  app.store.getState().edit('pattern', '([0-9]+)');
  const obsolete = await app.next();
  app.store.getState().edit('digits', '3');
  const current = await app.next();
  current.complete(proposed(1, 'S002E001.mp4'));
  obsolete.fail('Error anterior');
  await Promise.resolve();
  assert.deepEqual(app.screen().view, proposed(1, 'S002E001.mp4').preview);
});
