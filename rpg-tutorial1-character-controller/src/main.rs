//! Work-in-progress!

use crate::{level::Level, player::Player};
use rg3d::event::{DeviceEvent, DeviceId, WindowEvent};
use rg3d::{
    core::{color::Color, futures::executor::block_on, pool::Handle},
    engine::framework::{Framework, GameEngine, GameState},
    scene::Scene,
};

mod level;
mod player;

struct Game {
    scene: Handle<Scene>,
    level: Level,
    player: Player,
}

impl GameState for Game {
    fn init(engine: &mut GameEngine) -> Self
    where
        Self: Sized,
    {
        let mut scene = Scene::new();

        scene.ambient_lighting_color = Color::opaque(150, 150, 150);

        let player = block_on(Player::new(engine.resource_manager.clone(), &mut scene));

        Self {
            player,
            level: block_on(Level::new(engine.resource_manager.clone(), &mut scene)),
            scene: engine.scenes.add(scene),
        }
    }

    fn on_tick(&mut self, engine: &mut GameEngine, dt: f32) {
        let scene = &mut engine.scenes[self.scene];

        self.player.update(scene, dt);
    }

    fn on_device_event(
        &mut self,
        _engine: &mut GameEngine,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        self.player.handle_device_event(&event);
    }

    fn on_window_event(&mut self, _engine: &mut GameEngine, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { input, .. } => {
                self.player.handle_key_event(&input);
            }
            _ => (),
        }
    }
}

fn main() {
    Framework::<Game>::new().unwrap().title("RPG").run()
}
