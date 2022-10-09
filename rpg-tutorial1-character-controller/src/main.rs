use crate::{level::Level, player::Player};
use fyrox::{
    core::{color::Color, futures::executor::block_on, pool::Handle},
    engine::executor::Executor,
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
    plugin::{Plugin, PluginConstructor, PluginContext},
    scene::Scene,
};

mod level;
mod player;

struct Game {
    scene: Handle<Scene>,
    level: Level,
    player: Player,
}

struct GameConstructor;

impl PluginConstructor for GameConstructor {
    fn create_instance(&self, _: Handle<Scene>, context: PluginContext) -> Box<dyn Plugin> {
        Box::new(Game::new(context))
    }
}

impl Game {
    fn new(context: PluginContext) -> Self {
        let mut scene = Scene::new();

        scene.ambient_lighting_color = Color::opaque(150, 150, 150);

        let player = block_on(Player::new(context.resource_manager.clone(), &mut scene));

        Self {
            player,
            level: block_on(Level::new(context.resource_manager.clone(), &mut scene)),
            scene: context.scenes.add(scene),
        }
    }
}

impl Plugin for Game {
    fn update(&mut self, context: &mut PluginContext, _: &mut ControlFlow) {
        let scene = &mut context.scenes[self.scene];

        self.player.update(scene, context.dt);
    }

    fn on_os_event(
        &mut self,
        event: &Event<()>,
        _context: PluginContext,
        _control_flow: &mut ControlFlow,
    ) {
        match event {
            Event::DeviceEvent { event, .. } => {
                self.player.handle_device_event(&event);
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput { input, .. } => {
                    self.player.handle_key_event(&input);
                }
                _ => (),
            },
            _ => (),
        }
    }
}

fn main() {
    let mut executor = Executor::new();
    executor.add_plugin_constructor(GameConstructor);
    executor.get_window().set_title("RPG");
    executor.run();
}
