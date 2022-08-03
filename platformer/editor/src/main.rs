//! Editor with your game connected to it as a plugin.
use fyrox::{
    event_loop::EventLoop,
    gui::inspector::editors::{
        collection::VecCollectionPropertyEditorDefinition,
        inspectable::InspectablePropertyEditorDefinition,
    },
};
use fyroxed_base::{Editor, StartupData};
use platformer::{Animation, GameConstructor, KeyFrameTexture};

fn main() {
    let event_loop = EventLoop::new();
    let mut editor = Editor::new(
        &event_loop,
        Some(StartupData {
            working_directory: Default::default(),
            scene: "data/scene.rgs".into(),
        }),
    );
    editor.add_game_plugin(GameConstructor);

    // Register property editors here.
    let property_editors = &editor.inspector.property_editors;
    property_editors.insert(InspectablePropertyEditorDefinition::<KeyFrameTexture>::new());
    property_editors.insert(InspectablePropertyEditorDefinition::<Animation>::new());
    property_editors.insert(VecCollectionPropertyEditorDefinition::<KeyFrameTexture>::new());
    property_editors.insert(VecCollectionPropertyEditorDefinition::<Animation>::new());

    editor.run(event_loop)
}
