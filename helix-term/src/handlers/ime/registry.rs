use std::collections::HashMap;

use helix_view::{document::Mode, editor::Editor, DocumentId, ViewId};
use once_cell::sync::Lazy;
use parking_lot::Mutex;

use super::engine::ImeContext;

/// Global IME context registry.
///
/// Performance considerations:
/// - Uses `parking_lot::Mutex` for efficient locking (faster than std::sync::Mutex)
/// - Lock is held for minimal time (only for HashMap lookups/updates)
/// - IME state updates are infrequent (only on cursor move and mode switch)
/// - Contexts are small (Copy types), so operations are fast
static REGISTRY: Lazy<Mutex<ImeRegistry>> = Lazy::new(|| Mutex::new(ImeRegistry::default()));

#[derive(Default)]
struct ImeRegistry {
    // Use (DocumentId, ViewId) as key to ensure IME state independence per document+view combination
    contexts: HashMap<(DocumentId, ViewId), ImeContext>,
}

impl ImeRegistry {
    fn ensure_context(
        &mut self,
        doc_id: DocumentId,
        view_id: ViewId,
        mode: Mode,
    ) -> &mut ImeContext {
        self.contexts
            .entry((doc_id, view_id))
            .or_insert_with(|| ImeContext::new(mode))
    }

    fn prune_orphans(&mut self, editor: &Editor) {
        // Remove contexts for views that no longer exist
        // and for documents that no longer exist
        let valid_views: std::collections::HashSet<_> =
            editor.tree.views().map(|(v, _)| v.id).collect();
        let valid_docs: std::collections::HashSet<_> = editor.documents.keys().copied().collect();

        self.contexts.retain(|(doc_id, view_id), _| {
            valid_views.contains(view_id) && valid_docs.contains(doc_id)
        });
    }

    fn remove_document(&mut self, doc_id: DocumentId) {
        // Remove all contexts for a document when it's closed
        self.contexts.retain(|(d, _), _| *d != doc_id);
    }
}

/// Access IME context for a specific document+view combination.
///
/// This function holds the registry lock for the duration of the closure.
/// The lock is held for minimal time as operations are fast (HashMap lookup/update).
pub fn with_context_mut<R>(
    doc_id: DocumentId,
    view_id: ViewId,
    mode: Mode,
    f: impl FnOnce(&mut ImeContext) -> R,
) -> R {
    let mut registry = REGISTRY.lock();
    let ctx = registry.ensure_context(doc_id, view_id, mode);
    f(ctx)
}

pub fn prune_orphans(editor: &Editor) {
    let mut registry = REGISTRY.lock();
    registry.prune_orphans(editor);
}

/// Remove all IME contexts for a document when it's closed
pub fn remove_document(doc_id: DocumentId) {
    let mut registry = REGISTRY.lock();
    registry.remove_document(doc_id);
}

/// Get IME context for a document+view combination (for logging and testing purposes)
#[allow(dead_code)] // Used by integration tests (`helix-term/tests/test/ime.rs`)
pub fn context(doc_id: DocumentId, view_id: ViewId) -> Option<ImeContext> {
    let registry = REGISTRY.lock();
    registry.contexts.get(&(doc_id, view_id)).copied()
}
