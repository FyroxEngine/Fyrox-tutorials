//! Game project.
use fyrox::{
    core::{
        pool::Handle,
        uuid::{uuid, Uuid},
    },
    event::Event,
    plugin::{Plugin, PluginContext, PluginRegistrationContext},
    scene::Scene,
};

pub struct Game {
    scene: Handle<Scene>,
}

impl Game {
    pub fn new() -> Self {
        Self {
            scene: Default::default(),
        }
    }

    fn set_scene(&mut self, scene: Handle<Scene>, _context: PluginContext) {
        self.scene = scene;

        // Do additional actions with scene here.
    }
}

impl Plugin for Game {
    fn on_register(&mut self, _context: PluginRegistrationContext) {
        // Register your scripts here.
    }

    fn on_standalone_init(&mut self, context: PluginContext) {
        self.set_scene(context.scenes.add(Scene::new()), context);
    }

    fn on_enter_play_mode(&mut self, scene: Handle<Scene>, context: PluginContext) {
        // Obtain scene from the editor.
        self.set_scene(scene, context);
    }

    fn on_leave_play_mode(&mut self, context: PluginContext) {
        self.set_scene(Handle::NONE, context)
    }

    fn update(&mut self, _context: &mut PluginContext) {
        // Add your global update code here.
    }

    fn id(&self) -> Uuid {
        // Ideally this should be unique per-project.
        uuid!("cb358b1c-fc23-4c44-9e59-0a9671324196")
    }

    fn on_os_event(&mut self, _event: &Event<()>, _context: PluginContext) {
        // Do something on OS event here.
    }

    fn on_unload(&mut self, _context: &mut PluginContext) {
        // Do a cleanup here.
    }
}
