//! Game project.
use fyrox::{
    core::{
        inspect::{Inspect, PropertyInfo},
        pool::Handle,
        uuid::{uuid, Uuid},
        visitor::prelude::*,
    },
    event::Event,
    fxhash::FxHashMap,
    gui::inspector::{FieldKind, PropertyChanged},
    plugin::{Plugin, PluginContext, PluginRegistrationContext},
    scene::{
        node::{Node, TypeUuidProvider},
        Scene,
    },
    script::{ScriptContext, ScriptTrait},
};

pub struct Game {
    scene: Handle<Scene>,
}

impl TypeUuidProvider for Game {
    fn type_uuid() -> Uuid {
        // Ideally this should be unique per-project.
        uuid!("cb358b1c-fc23-4c44-9e59-0a9671324196")
    }
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
    fn on_register(&mut self, context: PluginRegistrationContext) {
        let script_constructors = &context.serialization_context.script_constructors;
        script_constructors.add::<Game, Player, _>("Player");
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
        Self::type_uuid()
    }

    fn on_os_event(&mut self, _event: &Event<()>, _context: PluginContext) {
        // Do something on OS event here.
    }

    fn on_unload(&mut self, _context: &mut PluginContext) {
        // Do a cleanup here.
    }
}

#[derive(Visit, Inspect, Debug, Clone)]
struct Player {
    body: Handle<Node>,
}

impl Default for Player {
    fn default() -> Self {
        Self { body: Handle::NONE }
    }
}

impl TypeUuidProvider for Player {
    // Returns unique script id for serialization needs.
    fn type_uuid() -> Uuid {
        uuid!("c5671d19-9f1a-4286-8486-add4ebaadaec")
    }
}

impl ScriptTrait for Player {
    // Accepts events from Inspector in the editor and modifies self state accordingly.
    fn on_property_changed(&mut self, args: &PropertyChanged) -> bool {
        if let FieldKind::Object(ref value) = args.value {
            match args.name.as_ref() {
                Player::BODY => {
                    self.body = value.cast_clone().unwrap();
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }

    // Called once at initialization.
    fn on_init(&mut self, context: ScriptContext) {}

    // Called everytime when there is an event from OS (mouse click, key press, etc.)
    fn on_os_event(&mut self, event: &Event<()>, context: ScriptContext) {}

    // Called every frame at fixed rate of 60 FPS.
    fn on_update(&mut self, context: ScriptContext) {}

    // Returns unique script id for serialization needs.
    fn id(&self) -> Uuid {
        Self::type_uuid()
    }

    // Returns unique id of parent plugin.
    fn plugin_uuid(&self) -> Uuid {
        Game::type_uuid()
    }
}
