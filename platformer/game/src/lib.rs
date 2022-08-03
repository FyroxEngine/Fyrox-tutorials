//! Game project.
use fyrox::plugin::PluginConstructor;
use fyrox::{
    core::{
        algebra::{Vector2, Vector3},
        futures::executor::block_on,
        inspect::{Inspect, PropertyInfo},
        pool::Handle,
        reflect::Reflect,
        uuid::{uuid, Uuid},
        visitor::prelude::*,
    },
    engine::resource_manager::ResourceManager,
    event::{ElementState, Event, VirtualKeyCode, WindowEvent},
    impl_component_provider,
    plugin::{Plugin, PluginContext, PluginRegistrationContext},
    resource::texture::Texture,
    scene::{
        dim2::{rectangle::Rectangle, rigidbody::RigidBody},
        node::{Node, TypeUuidProvider},
        Scene, SceneLoader,
    },
    script::{ScriptContext, ScriptTrait},
};

pub struct GameConstructor;

impl TypeUuidProvider for GameConstructor {
    fn type_uuid() -> Uuid {
        // Ideally this should be unique per-project.
        uuid!("cb358b1c-fc23-4c44-9e59-0a9671324196")
    }
}

impl PluginConstructor for GameConstructor {
    fn register(&self, context: PluginRegistrationContext) {
        let script_constructors = &context.serialization_context.script_constructors;
        script_constructors.add::<Player>("Player");
    }

    fn create_instance(
        &self,
        override_scene: Handle<Scene>,
        context: PluginContext,
    ) -> Box<dyn Plugin> {
        Box::new(Game::new(override_scene, context))
    }
}

pub struct Game {
    scene: Handle<Scene>,
}

impl Game {
    pub fn new(override_scene: Handle<Scene>, context: PluginContext) -> Self {
        let scene = if override_scene.is_some() {
            override_scene
        } else {
            let scene = block_on(
                block_on(SceneLoader::from_file(
                    "data/scene.rgs",
                    context.serialization_context.clone(),
                ))
                .unwrap()
                .finish(context.resource_manager.clone()),
            );

            context.scenes.add(scene)
        };

        Self { scene }
    }
}

impl Plugin for Game {
    fn id(&self) -> Uuid {
        GameConstructor::type_uuid()
    }
}

#[derive(Visit, Inspect, Reflect, Debug, Clone)]
struct Player {
    sprite: Handle<Node>,
    move_left: bool,
    move_right: bool,
    jump: bool,
    animations: Vec<Animation>,
    current_animation: u32,
}

impl_component_provider!(Player,);

impl Default for Player {
    fn default() -> Self {
        Self {
            sprite: Handle::NONE,
            move_left: false,
            move_right: false,
            jump: false,
            animations: Default::default(),
            current_animation: 0,
        }
    }
}

impl TypeUuidProvider for Player {
    // Returns unique script id for serialization needs.
    fn type_uuid() -> Uuid {
        uuid!("c5671d19-9f1a-4286-8486-add4ebaadaec")
    }
}

impl ScriptTrait for Player {
    // Called everytime when there is an event from OS (mouse click, key press, etc.)
    fn on_os_event(&mut self, event: &Event<()>, _context: ScriptContext) {
        if let Event::WindowEvent { event, .. } = event {
            if let WindowEvent::KeyboardInput { input, .. } = event {
                if let Some(keycode) = input.virtual_keycode {
                    let is_pressed = input.state == ElementState::Pressed;

                    match keycode {
                        VirtualKeyCode::A => self.move_left = is_pressed,
                        VirtualKeyCode::D => self.move_right = is_pressed,
                        VirtualKeyCode::Space => self.jump = is_pressed,
                        _ => (),
                    }
                }
            }
        }
    }

    fn restore_resources(&mut self, resource_manager: ResourceManager) {
        for animation in self.animations.iter_mut() {
            animation.restore_resources(resource_manager.clone());
        }
    }

    // Called every frame at fixed rate of 60 FPS.
    fn on_update(&mut self, context: ScriptContext) {
        // The script can be assigned to any scene node, but we assert that it will work only with
        // 2d rigid body nodes.
        if let Some(rigid_body) = context.scene.graph[context.handle].cast_mut::<RigidBody>() {
            let x_speed = if self.move_left {
                3.0
            } else if self.move_right {
                -3.0
            } else {
                0.0
            };

            if x_speed != 0.0 {
                self.current_animation = 0;
            } else {
                self.current_animation = 1;
            }

            if self.jump {
                rigid_body.set_lin_vel(Vector2::new(x_speed, 4.0))
            } else {
                rigid_body.set_lin_vel(Vector2::new(x_speed, rigid_body.lin_vel().y))
            };

            // It is always a good practice to check whether the handles are valid, at this point we don't know
            // for sure what's the value of the `sprite` field. It can be unassigned and the following code won't
            // execute. A simple `context.scene.graph[self.sprite]` would just panicked in this case.
            if let Some(sprite) = context.scene.graph.try_get_mut(self.sprite) {
                // We want to change player orientation only if he's moving.
                if x_speed != 0.0 {
                    let local_transform = sprite.local_transform_mut();

                    let current_scale = **local_transform.scale();

                    local_transform.set_scale(Vector3::new(
                        // Just change X scaling to mirror player's sprite.
                        current_scale.x.copysign(-x_speed),
                        current_scale.y,
                        current_scale.z,
                    ));
                }
            }
        }

        if let Some(current_animation) = self.animations.get_mut(self.current_animation as usize) {
            current_animation.update(context.dt);

            if let Some(sprite) = context
                .scene
                .graph
                .try_get_mut(self.sprite)
                .and_then(|n| n.cast_mut::<Rectangle>())
            {
                // Set new frame to the sprite.
                sprite.set_texture(
                    current_animation
                        .current_frame()
                        .and_then(|k| k.texture.clone()),
                );
            }
        }
    }

    // Returns unique script id for serialization needs.
    fn id(&self) -> Uuid {
        Self::type_uuid()
    }

    // Returns unique id of parent plugin.
    fn plugin_uuid(&self) -> Uuid {
        GameConstructor::type_uuid()
    }
}

#[derive(Default, Inspect, Reflect, Visit, Debug, Clone)]
pub struct KeyFrameTexture {
    texture: Option<Texture>,
}

impl KeyFrameTexture {
    fn restore_resources(&mut self, resource_manager: ResourceManager) {
        // It is very important to restore texture handle after loading, otherwise the handle will
        // remain in "shallow" state when it just has path to data, but not the actual resource handle.
        resource_manager
            .state()
            .containers_mut()
            .textures
            .try_restore_optional_resource(&mut self.texture);
    }
}

#[derive(Inspect, Visit, Reflect, Debug, Clone)]
pub struct Animation {
    name: String,
    keyframes: Vec<KeyFrameTexture>,
    current_frame: u32,
    speed: f32,

    // We don't want this field to be visible from the editor, because this is internal parameter.
    #[inspect(skip)]
    t: f32,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            name: "Unnamed".to_string(),
            keyframes: vec![],
            current_frame: 0,
            speed: 10.0,
            t: 0.0,
        }
    }
}

impl Animation {
    pub fn current_frame(&self) -> Option<&KeyFrameTexture> {
        self.keyframes.get(self.current_frame as usize)
    }

    fn restore_resources(&mut self, resource_manager: ResourceManager) {
        for key_frame in self.keyframes.iter_mut() {
            key_frame.restore_resources(resource_manager.clone());
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.t += self.speed * dt;

        if self.t >= 1.0 {
            self.t = 0.0;

            // Increase frame index and make sure it will be clamped in available bounds.
            self.current_frame = (self.current_frame + 1) % self.keyframes.len() as u32;
        }
    }
}
