//! Editor with your game connected to it as a plugin.
use fyrox::event_loop::EventLoop;
use fyroxed_base::{Editor, StartupData};
use platformer::Game;

fn main() {
    let event_loop = EventLoop::new();
    let mut editor = Editor::new(
        &event_loop,
        Some(StartupData {
            working_directory: Default::default(),
            // Set this to `"path/to/your/scene.rgs".into()` to force the editor to load the scene on startup.
            scene: Default::default(),
        }),
    );
    editor.add_game_plugin(Game::new());
    editor.run(event_loop)
}
