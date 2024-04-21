"""use helix_event::{hook, register_hook};
use helix_view::editor::Editor;
use helix_view::events::{SelectionDidChange, ModeSwitch};

fn update_ime_state(editor: &mut Editor) {
    editor.update_ime_state();
}

pub fn register_hooks() {
    register_hook!(move |editor: &mut Editor, _event: &mut SelectionDidChange<'_>| {
        update_ime_state(editor);
    });
    register_hook!(move |editor: &mut Editor, _event: &mut ModeSwitch<'_>| {
        update_ime_state(editor);
    });
}
""