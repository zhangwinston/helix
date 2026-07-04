use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use helix_view::ViewId;
use once_cell::sync::Lazy;
use tokio::time::sleep;

use super::handle_cursor_move;

const CURSOR_MOVE_BUFFER: Duration = Duration::from_millis(50);

static PENDING_VIEWS: Lazy<DashMap<ViewId, Arc<PendingState>>> = Lazy::new(DashMap::default);

struct PendingState {
    sequence: AtomicU64,
    worker_running: AtomicBool,
}

impl PendingState {
    fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
            worker_running: AtomicBool::new(false),
        }
    }
}

pub(super) fn schedule(view_id: ViewId) {
    let state = PENDING_VIEWS
        .entry(view_id)
        .or_insert_with(|| Arc::new(PendingState::new()))
        .clone();

    state.sequence.fetch_add(1, Ordering::Release);

    if state.worker_running.swap(true, Ordering::AcqRel) {
        return;
    }

    spawn_worker(view_id, state);
}

pub(super) fn cancel(view_id: ViewId) {
    if let Some((_, state)) = PENDING_VIEWS.remove(&view_id) {
        state.worker_running.store(false, Ordering::Release);
    }
}

fn spawn_worker(view_id: ViewId, state: Arc<PendingState>) {
    tokio::spawn(async move {
        loop {
            let observed_seq = state.sequence.load(Ordering::Acquire);

            sleep(CURSOR_MOVE_BUFFER).await;

            if state.sequence.load(Ordering::Acquire) != observed_seq {
                continue;
            }

            let view_alive = Arc::new(AtomicBool::new(true));
            let view_alive_flag = view_alive.clone();
            let view_id_copy = view_id;
            crate::job::dispatch_blocking(move |editor, _| {
                if !editor.tree.contains(view_id_copy) {
                    view_alive_flag.store(false, Ordering::Release);
                    return;
                }

                if let Err(e) = handle_cursor_move(editor, view_id_copy) {
                    log::error!("Failed to handle cursor move for IME: {}", e);
                }
            });

            if !view_alive.load(Ordering::Acquire) {
                state.worker_running.store(false, Ordering::Release);
                cancel(view_id);
                break;
            }

            if state.sequence.load(Ordering::Acquire) == observed_seq {
                if state
                    .worker_running
                    .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // Check if new events arrived after releasing the flag.
                    if state.sequence.load(Ordering::Acquire) == observed_seq {
                        break;
                    }

                    if state.worker_running.swap(true, Ordering::AcqRel) {
                        break;
                    }

                    continue;
                }
            }
        }
    });
}
