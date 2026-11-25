use renombrador_core::{
    application::{EditPreview, PreviewRequest, Revision, calculate_preview},
    infrastructure::folder::{NativeFolder, NativeSelection},
};
use serde::Serialize;
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, MutexGuard},
};
use tauri::async_runtime::{Receiver, Sender, channel, spawn, spawn_blocking};

const PREVIEW_FAILURE: &str = "No se pudo calcular la vista previa.";

#[derive(Serialize)]
pub struct Selection {
    revision: u32,
    files: Vec<SelectedFile>,
}

#[derive(Serialize)]
struct SelectedFile {
    id: String,
    name: String,
    size: u64,
    kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResponse {
    selection_revision: u32,
    edit_revision: u32,
    #[serde(flatten)]
    preview: EditPreview,
}

impl PreviewResponse {
    fn new(revision: Revision, preview: EditPreview) -> Self {
        Self {
            selection_revision: revision.selection_revision,
            edit_revision: revision.edit_revision,
            preview,
        }
    }
}

type Response = Result<PreviewResponse, &'static str>;

struct Work {
    selection: Arc<NativeSelection>,
    request: PreviewRequest,
    result: Sender<Response>,
}

#[derive(Clone, Default)]
pub struct SelectionState(Arc<Mutex<Queue>>);

#[derive(Default)]
struct Queue {
    selection: Option<Arc<NativeSelection>>,
    selection_revision: u32,
    edit_revision: u32,
    last_submitted: Option<u32>,
    active: bool,
    pending: Option<Work>,
}

fn reply(result: &Sender<Response>, revision: Revision, preview: EditPreview) {
    let _ = result.try_send(Ok(PreviewResponse::new(revision, preview)));
}

impl Queue {
    fn current(&self, revision: Revision) -> bool {
        revision.selection_revision == self.selection_revision
            && revision.edit_revision == self.edit_revision
    }

    fn dismiss_pending(&mut self) {
        if let Some(work) = self.pending.take() {
            reply(&work.result, work.request.revision, EditPreview::Stale);
        }
    }

    fn advance(&mut self, edit_revision: u32) {
        if edit_revision > self.edit_revision {
            self.edit_revision = edit_revision;
            self.dismiss_pending();
        }
    }
}

impl SelectionState {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn replace(&self, selection: NativeSelection) -> Selection {
        let files = selection
            .files
            .iter()
            .map(|file| SelectedFile {
                id: file.id.clone(),
                name: file.name.clone(),
                size: file.size,
                kind: file.kind.clone(),
            })
            .collect();
        let mut queue = self.lock();
        queue.dismiss_pending();
        queue.selection_revision = queue.selection_revision.wrapping_add(1);
        queue.edit_revision = 0;
        queue.last_submitted = None;
        queue.selection = Some(Arc::new(selection));
        Selection {
            revision: queue.selection_revision,
            files,
        }
    }

    pub fn notify(&self, revision: Revision) {
        let mut queue = self.lock();
        if revision.selection_revision == queue.selection_revision && queue.selection.is_some() {
            queue.advance(revision.edit_revision);
        }
    }

    pub async fn preview(&self, request: PreviewRequest) -> Response {
        let (mut response, work) = self.enqueue(request)?;
        if let Some(work) = work {
            let state = self.clone();
            spawn(async move { state.run(work).await });
        }
        response.recv().await.unwrap_or(Err(PREVIEW_FAILURE))
    }

    fn enqueue(
        &self,
        request: PreviewRequest,
    ) -> Result<(Receiver<Response>, Option<Work>), &'static str> {
        let mut queue = self.lock();
        let revision = request.revision;
        let (result, response) = channel(1);
        let selected = queue.selection.as_ref().filter(|_| {
            revision.selection_revision == queue.selection_revision
                && revision.edit_revision >= queue.edit_revision
        });
        let Some(selected) = selected else {
            reply(&result, revision, EditPreview::Stale);
            return Ok((response, None));
        };
        if !request.excluded_ids.is_empty() {
            let mut unchecked = request.excluded_ids.iter().collect::<HashSet<_>>();
            for file in &selected.files {
                unchecked.remove(&file.id);
                if unchecked.is_empty() {
                    break;
                }
            }
            if !unchecked.is_empty() {
                return Err("La vista previa incluye un identificador ajeno a la selección.");
            }
        }
        if !request.manual_names.is_empty() || request.draft_file_id.is_some() {
            let known = selected
                .files
                .iter()
                .map(|file| file.id.as_str())
                .collect::<HashSet<_>>();
            let mut names = HashSet::new();
            for correction in &request.manual_names {
                if !known.contains(correction.file_id.as_str())
                    || !names.insert(correction.file_id.as_str())
                {
                    return Err(
                        "La vista previa incluye una corrección repetida o ajena a la selección.",
                    );
                }
            }
            if let Some(id) = &request.draft_file_id
                && (!names.contains(id.as_str()) || request.excluded_ids.contains(id))
            {
                return Err(
                    "El borrador debe corresponder a una corrección de un archivo incluido.",
                );
            }
        }
        if queue.last_submitted == Some(revision.edit_revision) {
            reply(&result, revision, EditPreview::Stale);
            return Ok((response, None));
        }
        let selection = selected.clone();
        queue.advance(revision.edit_revision);
        queue.last_submitted = Some(revision.edit_revision);
        let work = Work {
            selection,
            request,
            result,
        };
        if queue.active {
            queue.pending = Some(work);
            Ok((response, None))
        } else {
            queue.active = true;
            Ok((response, Some(work)))
        }
    }

    async fn run(&self, mut work: Work) {
        loop {
            let Work {
                selection,
                request,
                result,
            } = work;
            let revision = request.revision;
            let state = self.clone();
            let outcome = spawn_blocking(move || {
                let current = || state.lock().current(revision);
                calculate_preview(
                    &selection.files,
                    &request,
                    &mut NativeFolder::new(&selection),
                    &current,
                )
            })
            .await
            .map_err(|_| PREVIEW_FAILURE);
            let Some(next) = self.complete(revision, &result, outcome) else {
                return;
            };
            work = next;
        }
    }

    fn complete(
        &self,
        revision: Revision,
        result: &Sender<Response>,
        outcome: Result<EditPreview, &'static str>,
    ) -> Option<Work> {
        let mut queue = self.lock();
        if queue.current(revision) {
            let _ = result.try_send(outcome.map(|preview| PreviewResponse::new(revision, preview)));
        } else {
            reply(result, revision, EditPreview::Stale);
        }
        let next = queue.pending.take();
        queue.active = next.is_some();
        next
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use renombrador_core::{
        domain::{
            order::Sort,
            rules::{EditRule, Extension, RuleConfig},
        },
        infrastructure::selection::read_selection,
    };
    use std::{
        fs,
        path::PathBuf,
        sync::{
            Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "renamancer-queue-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::write(path.join("video.1.mp4"), "contenido").unwrap();
            Self(path)
        }

        fn selection(&self) -> NativeSelection {
            NativeSelection::capture(read_selection(&[self.0.join("video.1.mp4")]).unwrap())
                .unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_file(self.0.join("video.1.mp4"));
            let _ = fs::remove_dir(&self.0);
        }
    }

    fn request(selection_revision: u32, edit_revision: u32) -> PreviewRequest {
        PreviewRequest {
            revision: Revision {
                selection_revision,
                edit_revision,
            },
            config: EditRule::Episode {
                rule: RuleConfig::default(),
                extension: Extension::Keep,
            },
            sort: Sort::default(),
            excluded_ids: Vec::new(),
            manual_names: Vec::new(),
            draft_file_id: None,
        }
    }

    fn is_stale(receiver: &mut Receiver<Response>) -> bool {
        matches!(
            receiver.try_recv(),
            Ok(Ok(PreviewResponse {
                preview: EditPreview::Stale,
                ..
            }))
        )
    }

    #[test]
    fn replaces_pending_only_after_validation_and_waits_for_the_active_calculation() {
        let state = SelectionState::default();
        let fixture = Fixture::new();
        let selection = state.replace(fixture.selection()).revision;
        let (mut first_result, first_work) = state.enqueue(request(selection, 10)).unwrap();
        let first_work = first_work.unwrap();
        let barrier = Arc::new(Barrier::new(2));
        let worker = {
            let state = state.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                barrier.wait();
                state
                    .complete(
                        first_work.request.revision,
                        &first_work.result,
                        Err(PREVIEW_FAILURE),
                    )
                    .unwrap()
            })
        };

        let (mut middle_result, middle_work) = state.enqueue(request(selection, 11)).unwrap();
        assert!(middle_work.is_none());
        let mut invalid = request(selection, 12);
        invalid.excluded_ids.push("ajeno".into());
        assert!(state.enqueue(invalid).is_err());
        assert_eq!(
            state
                .lock()
                .pending
                .as_ref()
                .unwrap()
                .request
                .revision
                .edit_revision,
            11
        );
        assert_eq!(state.lock().edit_revision, 11);
        let (mut last_result, last_work) = state.enqueue(request(selection, 12)).unwrap();
        assert!(last_work.is_none());
        assert!(is_stale(&mut middle_result));
        let (mut late, _) = state.enqueue(request(selection, 11)).unwrap();
        assert!(is_stale(&mut late));

        barrier.wait();
        let last = worker.join().unwrap();
        assert_eq!(last.request.revision.edit_revision, 12);
        assert!(is_stale(&mut first_result));
        assert!(
            state
                .complete(last.request.revision, &last.result, Err(PREVIEW_FAILURE))
                .is_none()
        );
        assert!(matches!(last_result.try_recv(), Ok(Err(PREVIEW_FAILURE))));
        assert!(!state.lock().active);
    }

    #[test]
    fn notices_can_precede_requests_and_selection_replacement_invalidates_work() {
        let state = SelectionState::default();
        let fixture = Fixture::new();
        let selection = state.replace(fixture.selection()).revision;
        state.notify(request(selection, 1).revision);
        let (mut active, first) = state.enqueue(request(selection, 1)).unwrap();
        let first = first.unwrap();
        state.notify(first.request.revision);
        assert!(state.lock().current(first.request.revision));

        let (mut pending, work) = state.enqueue(request(selection, 2)).unwrap();
        assert!(work.is_none());
        state.notify(request(selection, 3).revision);
        assert!(is_stale(&mut pending));
        let (mut pending, _) = state.enqueue(request(selection, 3)).unwrap();
        let new_selection = state.replace(fixture.selection()).revision;
        assert!(is_stale(&mut pending));
        let (mut old, _) = state.enqueue(request(selection, 4)).unwrap();
        assert!(is_stale(&mut old));
        let (mut new, _) = state.enqueue(request(new_selection, 1)).unwrap();
        let next = state
            .complete(
                first.request.revision,
                &first.result,
                Ok(EditPreview::Ready { rows: Vec::new() }),
            )
            .unwrap();
        assert!(is_stale(&mut active));
        assert_eq!(next.request.revision.selection_revision, new_selection);
        assert!(
            state
                .complete(
                    next.request.revision,
                    &next.result,
                    Ok(EditPreview::Ready { rows: Vec::new() })
                )
                .is_none()
        );
        assert!(matches!(
            new.try_recv(),
            Ok(Ok(PreviewResponse {
                preview: EditPreview::Ready { .. },
                ..
            }))
        ));
    }

    #[test]
    fn wire_payloads_reach_the_tauri_commands_and_native_preview() {
        use serde::Deserialize;
        use tauri::{
            WebviewWindowBuilder,
            ipc::{CallbackFn, InvokeBody},
            test::{INVOKE_KEY, get_ipc_response, mock_builder, mock_context, noop_assets},
            webview::InvokeRequest,
        };

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireRow {
            file_id: String,
            proposed_name: Option<String>,
            name_edit: Option<WireNameEdit>,
            retryable: bool,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireNameEdit {
            base: String,
            extension: String,
            source: String,
            can_save: bool,
            save_reason: Option<String>,
        }

        #[derive(Debug, Deserialize)]
        struct WireError {
            field: String,
        }

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireResponse {
            selection_revision: u32,
            edit_revision: u32,
            status: String,
            #[serde(default)]
            rows: Vec<WireRow>,
            #[serde(default)]
            errors: Vec<WireError>,
        }

        let fixture = Fixture::new();
        let before = fs::read(fixture.0.join("video.1.mp4")).unwrap();
        let state = SelectionState::default();
        assert_eq!(state.replace(fixture.selection()).revision, 1);
        let app = mock_builder()
            .manage(state)
            .invoke_handler(tauri::generate_handler![
                crate::commands::generate_preview,
                crate::commands::notify_preview_revision
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let invoke = |cmd: &str, payload: String| {
            get_ipc_response(
                &webview,
                InvokeRequest {
                    cmd: cmd.into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "http://tauri.localhost".parse().unwrap(),
                    body: InvokeBody::Json(payload.parse().unwrap()),
                    headers: Default::default(),
                    invoke_key: INVOKE_KEY.into(),
                },
            )
        };

        for (index, (config, name)) in [
            (r#"{"mode":"episode","extension":{"kind":"keep"},"season":"1","digits":"2","pattern":"([0-9]+)$","template":"S{temporada}E{episodio}{ext}"}"#, Some("S01E01.mp4")),
            (r#"{"mode":"regex","extension":{"kind":"change","value":"mkv"},"pattern":"video","replacement":"clip","ignoreCase":false}"#, Some("clip.1.mkv")),
            (r#"{"mode":"numbering","extension":{"kind":"keep"},"start":"0","step":"1","width":"2","template":"{n}"}"#, Some("00.mp4")),
            (r#"{"mode":"text","extension":{"kind":"remove"},"search":"video","replacement":"clip","prefix":"Nuevo-","suffix":"","capitalization":"keep"}"#, Some("Nuevo-clip.1")),
            (r#"{"mode":"numbering","extension":{"kind":"keep"},"start":"","step":"1","width":"2","template":"{n}"}"#, None),
        ].into_iter().enumerate() {
            let edit = index + 1;
            invoke("notify_preview_revision", format!(r#"{{"request":{{"selectionRevision":1,"editRevision":{edit}}}}}"#))
                .unwrap().deserialize::<()>().unwrap();
            let result = invoke("generate_preview", format!(
                r#"{{"request":{{"selectionRevision":1,"editRevision":{edit},"config":{config},"sort":{{"column":"name","direction":"asc"}},"excludedIds":[]}}}}"#
            )).unwrap().deserialize::<WireResponse>().unwrap();
            assert_eq!(result.selection_revision, 1);
            assert_eq!(result.edit_revision, edit as u32);
            assert_eq!(result.rows[0].file_id, "f0");
            assert_eq!(result.rows[0].proposed_name.as_deref(), name);
            assert!(!result.rows[0].retryable);
            if name.is_some() {
                assert_eq!(result.status, "ready", "{result:?}");
                assert!(!result.rows[0].name_edit.as_ref().unwrap().can_save);
            } else {
                assert_eq!(result.status, "invalid");
                assert_eq!(result.errors[0].field, "start");
                assert!(result.rows[0].name_edit.is_none());
            }
        }

        let preview = |edit, excluded, corrections| {
            invoke(
                "generate_preview",
                format!(
                    r#"{{"request":{{"selectionRevision":1,"editRevision":{edit},"config":{{"mode":"regex","extension":{{"kind":"remove"}},"pattern":"video","replacement":"clip","ignoreCase":false}},"sort":{{"column":"name","direction":"asc"}},"excludedIds":{excluded}{corrections}}}}}"#
                ),
            )
        };
        for bad in [
            r#","manualNames":null"#,
            r#","manualNames":[["f0","base"]]"#,
            r#","manualNames":[{"fileId":"f0","base":"base","extra":true}]"#,
            r#","manualNames":[{"fileId":"foreign","base":"base"}]"#,
            r#","manualNames":[{"fileId":"f0","base":"a"},{"fileId":"f0","base":"b"}]"#,
            r#","draftFileId":"f0""#,
        ] {
            assert!(preview(20, "[]", bad).is_err(), "{bad}");
        }
        let draft = r#","manualNames":[{"fileId":"f0","base":"revisado.v2"}],"draftFileId":"f0""#;
        assert!(preview(20, r#"["f0"]"#, draft).is_err());
        let result = preview(6, "[]", draft)
            .unwrap()
            .deserialize::<WireResponse>()
            .unwrap();
        assert_eq!(result.status, "ready");
        assert_eq!(result.rows[0].proposed_name.as_deref(), Some("revisado.v2"));
        let edit = result.rows[0].name_edit.as_ref().unwrap();
        assert_eq!(edit.base, "revisado.v2");
        assert_eq!(edit.extension, "");
        assert_eq!(edit.source, "manual");
        assert!(edit.can_save);
        assert!(edit.save_reason.is_none());
        let empty = r#","manualNames":[{"fileId":"f0","base":""}],"draftFileId":"f0""#;
        let result = preview(7, "[]", empty)
            .unwrap()
            .deserialize::<WireResponse>()
            .unwrap();
        assert!(result.rows[0].proposed_name.is_none());
        assert!(!result.rows[0].name_edit.as_ref().unwrap().can_save);
        let result = preview(8, "[]", r#","draftFileId":null"#)
            .unwrap()
            .deserialize::<WireResponse>()
            .unwrap();
        assert_eq!(result.rows[0].proposed_name.as_deref(), Some("clip.1"));

        invoke(
            "notify_preview_revision",
            r#"{"request":{"selectionRevision":1,"editRevision":10}}"#.into(),
        )
        .unwrap()
        .deserialize::<()>()
        .unwrap();
        assert!(preview(10, "[]", r#","draftFileId":"f0""#).is_err());
        let stale = preview(9, "[]", "")
            .unwrap()
            .deserialize::<WireResponse>()
            .unwrap();
        assert_eq!(stale.status, "stale");
        assert_eq!(fs::read(fixture.0.join("video.1.mp4")).unwrap(), before);
    }
}
