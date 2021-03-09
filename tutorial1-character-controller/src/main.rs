use rg3d::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    engine::{resource_manager::ResourceManager, Engine},
    event::{DeviceEvent, ElementState, Event, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    gui::node::StubNode,
    physics::{dynamics::RigidBodyBuilder, geometry::ColliderBuilder},
    resource::texture::TextureWrapMode,
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, SkyBox},
        node::Node,
        transform::TransformBuilder,
        RigidBodyHandle, Scene,
    },
    window::WindowBuilder,
};
use std::time;

// Create our own engine type aliases. These specializations are needed, because the engine
// provides a way to extend UI with custom nodes and messages.
type GameEngine = Engine<(), StubNode>;

// Our game logic will be updated at 60 Hz rate.
const TIMESTEP: f32 = 1.0 / 60.0;

#[derive(Default)]
struct InputController {
    move_forward: bool,
    move_backward: bool,
    move_left: bool,
    move_right: bool,
    pitch: f32,
    yaw: f32,
}

struct Player {
    pivot: Handle<Node>,
    camera: Handle<Node>,
    rigid_body: RigidBodyHandle,
    controller: InputController,
}

async fn create_skybox(resource_manager: ResourceManager) -> SkyBox {
    // Load skybox textures in parallel.
    let (front, back, left, right, top, bottom) = rg3d::futures::join!(
        resource_manager.request_texture("data/textures/skybox/front.jpg"),
        resource_manager.request_texture("data/textures/skybox/back.jpg"),
        resource_manager.request_texture("data/textures/skybox/left.jpg"),
        resource_manager.request_texture("data/textures/skybox/right.jpg"),
        resource_manager.request_texture("data/textures/skybox/up.jpg"),
        resource_manager.request_texture("data/textures/skybox/down.jpg")
    );

    // Unwrap everything.
    let skybox = SkyBox {
        front: Some(front.unwrap()),
        back: Some(back.unwrap()),
        left: Some(left.unwrap()),
        right: Some(right.unwrap()),
        top: Some(top.unwrap()),
        bottom: Some(bottom.unwrap()),
    };

    // Set S and T coordinate wrap mode, ClampToEdge will remove any possible seams on edges
    // of the skybox.
    for skybox_texture in skybox.textures().iter().filter_map(|t| t.clone()) {
        let mut data = skybox_texture.data_ref();
        data.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
        data.set_t_wrap_mode(TextureWrapMode::ClampToEdge);
    }

    skybox
}

impl Player {
    async fn new(scene: &mut Scene, resource_manager: ResourceManager) -> Self {
        // Create a pivot and attach a camera to it, move it a bit up to "emulate" head.
        let camera;
        let pivot = BaseBuilder::new()
            .with_children(&[{
                camera = CameraBuilder::new(
                    BaseBuilder::new().with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(Vector3::new(0.0, 0.25, 0.0))
                            .build(),
                    ),
                )
                .with_skybox(create_skybox(resource_manager).await)
                .build(&mut scene.graph);
                camera
            }])
            .build(&mut scene.graph);

        // Create rigid body, it will be used for interaction with the world.
        let rigid_body_handle = scene.physics.add_body(
            RigidBodyBuilder::new_dynamic()
                .lock_rotations() // We don't want the player to tilt.
                .translation(0.0, 1.0, -1.0) // Offset player a bit.
                .build(),
        );

        // Add capsule collider for the rigid body.
        scene.physics.add_collider(
            ColliderBuilder::capsule_y(0.25, 0.2).build(),
            rigid_body_handle,
        );

        // Bind pivot with rigid body. Scene will automatically sync transform of the pivot
        // with the transform of the rigid body.
        scene.physics_binder.bind(pivot, rigid_body_handle);

        Self {
            pivot,
            camera,
            rigid_body: rigid_body_handle.into(),
            controller: Default::default(),
        }
    }

    fn update(&mut self, scene: &mut Scene) {
        // Set pitch for the camera. These lines responsible for up-down camera rotation.
        scene.graph[self.camera].local_transform_mut().set_rotation(
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.controller.pitch.to_radians()),
        );

        // Borrow the pivot in the graph.
        let pivot = &mut scene.graph[self.pivot];

        // Borrow rigid body in the physics.
        let body = scene
            .physics
            .bodies
            .get_mut(self.rigid_body.into())
            .unwrap();

        // Keep only vertical velocity, and drop horizontal.
        let mut velocity = Vector3::new(0.0, body.linvel().y, 0.0);

        // Change the velocity depending on the keys pressed.
        if self.controller.move_forward {
            // If we moving forward then add "look" vector of the pivot.
            velocity += pivot.look_vector();
        }
        if self.controller.move_backward {
            // If we moving backward then subtract "look" vector of the pivot.
            velocity -= pivot.look_vector();
        }
        if self.controller.move_left {
            // If we moving left then add "side" vector of the pivot.
            velocity += pivot.side_vector();
        }
        if self.controller.move_right {
            // If we moving right then subtract "side" vector of the pivot.
            velocity -= pivot.side_vector();
        }

        // Finally new linear velocity.
        body.set_linvel(velocity, true);

        // Change the rotation of the rigid body according to current yaw. These lines responsible for
        // left-right rotation.
        let mut position = *body.position();
        position.rotation =
            UnitQuaternion::from_axis_angle(&Vector3::y_axis(), self.controller.yaw.to_radians());
        body.set_position(position, true);
    }

    fn process_input_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::KeyboardInput { input, .. } = event {
                    if let Some(key_code) = input.virtual_keycode {
                        match key_code {
                            VirtualKeyCode::W => {
                                self.controller.move_forward = input.state == ElementState::Pressed;
                            }
                            VirtualKeyCode::S => {
                                self.controller.move_backward =
                                    input.state == ElementState::Pressed;
                            }
                            VirtualKeyCode::A => {
                                self.controller.move_left = input.state == ElementState::Pressed;
                            }
                            VirtualKeyCode::D => {
                                self.controller.move_right = input.state == ElementState::Pressed;
                            }
                            _ => (),
                        }
                    }
                }
            }
            Event::DeviceEvent { event, .. } => {
                if let DeviceEvent::MouseMotion { delta } = event {
                    self.controller.yaw -= delta.0 as f32;

                    self.controller.pitch =
                        (self.controller.pitch + delta.1 as f32).clamp(-90.0, 90.0);
                }
            }
            _ => (),
        }
    }
}

struct Game {
    scene: Handle<Scene>,
    player: Player,
}

impl Game {
    pub async fn new(engine: &mut GameEngine) -> Self {
        let mut scene = Scene::new();

        // Load a scene resource and create its instance.
        engine
            .resource_manager
            .request_model("data/models/scene.rgs")
            .await
            .unwrap()
            .instantiate_geometry(&mut scene);

        Self {
            player: Player::new(&mut scene, engine.resource_manager.clone()).await,
            scene: engine.scenes.add(scene),
        }
    }

    pub fn update(&mut self, engine: &mut GameEngine) {
        self.player.update(&mut engine.scenes[self.scene]);
    }
}

fn main() {
    // Configure main window first.
    let window_builder = WindowBuilder::new().with_title("3D Shooter Tutorial");
    // Create event loop that will be used to "listen" events from the OS.
    let event_loop = EventLoop::new();

    // Finally create an instance of the engine.
    let mut engine = GameEngine::new(window_builder, &event_loop, true).unwrap();

    // Initialize game instance.
    let mut game = rg3d::futures::executor::block_on(Game::new(&mut engine));

    // Run the event loop of the main window. which will respond to OS and window events and update
    // engine's state accordingly. Engine lets you to decide which event should be handled,
    // this is minimal working example if how it should be.
    let clock = time::Instant::now();
    let mut elapsed_time = 0.0;
    event_loop.run(move |event, _, control_flow| {
        game.player.process_input_event(&event);

        match event {
            Event::MainEventsCleared => {
                // This main game loop - it has fixed time step which means that game
                // code will run at fixed speed even if renderer can't give you desired
                // 60 fps.
                let mut dt = clock.elapsed().as_secs_f32() - elapsed_time;
                while dt >= TIMESTEP {
                    dt -= TIMESTEP;
                    elapsed_time += TIMESTEP;

                    // Run our game's logic.
                    game.update(&mut engine);

                    // Update engine each frame.
                    engine.update(TIMESTEP);
                }

                // Rendering must be explicitly requested and handled after RedrawRequested event is received.
                engine.get_window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Render at max speed - it is not tied to the game code.
                engine.render(TIMESTEP).unwrap();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                WindowEvent::KeyboardInput { input, .. } => {
                    // Exit game by hitting Escape.
                    if let Some(VirtualKeyCode::Escape) = input.virtual_keycode {
                        *control_flow = ControlFlow::Exit
                    }
                }
                WindowEvent::Resized(size) => {
                    // It is very important to handle Resized event from window, because
                    // renderer knows nothing about window size - it must be notified
                    // directly when window size has changed.
                    engine.renderer.set_frame_size(size.into());
                }
                _ => (),
            },
            _ => *control_flow = ControlFlow::Poll,
        }
    });
}
