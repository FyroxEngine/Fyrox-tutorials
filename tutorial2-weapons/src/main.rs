use crate::{message::Message, weapon::Weapon};
use fyrox::{
    core::{
        algebra::{Point3, UnitQuaternion, Vector3},
        color::Color,
        color_gradient::{ColorGradient, GradientPoint},
        math::ray::Ray,
        math::vector_to_quat,
        parking_lot::Mutex,
        pool::{Handle, Pool},
        sstorage::ImmutableString,
    },
    engine::{resource_manager::ResourceManager, Engine, EngineInitParams, SerializationContext},
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    material::{Material, PropertyValue},
    resource::texture::TextureWrapMode,
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, SkyBox, SkyBoxBuilder},
        collider::{ColliderBuilder, ColliderShape},
        graph::{physics::RayCastOptions, Graph},
        mesh::{
            surface::{SurfaceBuilder, SurfaceData},
            MeshBuilder, RenderPath,
        },
        node::Node,
        particle_system::{
            emitter::{base::BaseEmitterBuilder, sphere::SphereEmitterBuilder},
            ParticleSystemBuilder,
        },
        pivot::PivotBuilder,
        rigidbody::RigidBodyBuilder,
        transform::TransformBuilder,
        Scene,
    },
    window::WindowBuilder,
};
use std::{
    path::Path,
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time,
};

pub mod message;
pub mod weapon;

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
    shoot: bool,
}

struct Player {
    camera: Handle<Node>,
    rigid_body: Handle<Node>,
    controller: InputController,
    weapon_pivot: Handle<Node>,
    sender: Sender<Message>,
    weapon: Handle<Weapon>,
    collider: Handle<Node>,
}

async fn create_skybox(resource_manager: ResourceManager) -> SkyBox {
    // Load skybox textures in parallel.
    let (front, back, left, right, top, bottom) = fyrox::core::futures::join!(
        resource_manager.request_texture("data/textures/skybox/front.jpg"),
        resource_manager.request_texture("data/textures/skybox/back.jpg"),
        resource_manager.request_texture("data/textures/skybox/left.jpg"),
        resource_manager.request_texture("data/textures/skybox/right.jpg"),
        resource_manager.request_texture("data/textures/skybox/up.jpg"),
        resource_manager.request_texture("data/textures/skybox/down.jpg")
    );

    // Unwrap everything.
    let skybox = SkyBoxBuilder {
        front: Some(front.unwrap()),
        back: Some(back.unwrap()),
        left: Some(left.unwrap()),
        right: Some(right.unwrap()),
        top: Some(top.unwrap()),
        bottom: Some(bottom.unwrap()),
    }
    .build()
    .unwrap();

    // Set S and T coordinate wrap mode, ClampToEdge will remove any possible seams on edges
    // of the skybox.
    let skybox_texture = skybox.cubemap().unwrap();
    let mut data = skybox_texture.data_ref();
    data.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
    data.set_t_wrap_mode(TextureWrapMode::ClampToEdge);

    skybox
}

fn create_bullet_impact(
    graph: &mut Graph,
    resource_manager: ResourceManager,
    pos: Vector3<f32>,
    orientation: UnitQuaternion<f32>,
) -> Handle<Node> {
    // Create sphere emitter first.
    let emitter = SphereEmitterBuilder::new(
        BaseEmitterBuilder::new()
            .with_max_particles(200)
            .with_spawn_rate(3000)
            .with_size_modifier_range(-0.01..-0.0125)
            .with_size_range(0.0075..0.015)
            .with_lifetime_range(0.05..0.2)
            .with_x_velocity_range(-0.0075..0.0075)
            .with_y_velocity_range(-0.0075..0.0075)
            .with_z_velocity_range(0.025..0.045)
            .resurrect_particles(false),
    )
    .with_radius(0.01)
    .build();

    // Color gradient will be used to modify color of each particle over its lifetime.
    let color_gradient = {
        let mut gradient = ColorGradient::new();
        gradient.add_point(GradientPoint::new(0.00, Color::from_rgba(255, 255, 0, 0)));
        gradient.add_point(GradientPoint::new(0.05, Color::from_rgba(255, 160, 0, 255)));
        gradient.add_point(GradientPoint::new(0.95, Color::from_rgba(255, 120, 0, 255)));
        gradient.add_point(GradientPoint::new(1.00, Color::from_rgba(255, 60, 0, 0)));
        gradient
    };

    // Create new transform to orient and position particle system.
    let transform = TransformBuilder::new()
        .with_local_position(pos)
        .with_local_rotation(orientation)
        .build();

    // Finally create particle system with limited lifetime.
    ParticleSystemBuilder::new(
        BaseBuilder::new()
            .with_lifetime(1.0)
            .with_local_transform(transform),
    )
    .with_acceleration(Vector3::new(0.0, 0.0, 0.0))
    .with_color_over_lifetime_gradient(color_gradient)
    .with_emitters(vec![emitter])
    // We'll use simple spark texture for each particle.
    .with_texture(resource_manager.request_texture(Path::new("data/textures/spark.png")))
    .build(graph)
}

impl Player {
    async fn new(
        scene: &mut Scene,
        resource_manager: ResourceManager,
        sender: Sender<Message>,
    ) -> Self {
        // Create rigid body with a camera, move it a bit up to "emulate" head.
        let camera;
        let weapon_pivot;
        let collider;
        let rigid_body_handle = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_local_transform(
                    TransformBuilder::new()
                        // Offset player a bit.
                        .with_local_position(Vector3::new(0.0, 1.0, -1.0))
                        .build(),
                )
                .with_children(&[
                    {
                        camera = CameraBuilder::new(
                            BaseBuilder::new()
                                .with_local_transform(
                                    TransformBuilder::new()
                                        .with_local_position(Vector3::new(0.0, 0.25, 0.0))
                                        .build(),
                                )
                                .with_children(&[{
                                    weapon_pivot = PivotBuilder::new(
                                        BaseBuilder::new().with_local_transform(
                                            TransformBuilder::new()
                                                .with_local_position(Vector3::new(
                                                    -0.1, -0.05, 0.015,
                                                ))
                                                .build(),
                                        ),
                                    )
                                    .build(&mut scene.graph);
                                    weapon_pivot
                                }]),
                        )
                        .with_skybox(create_skybox(resource_manager).await)
                        .build(&mut scene.graph);
                        camera
                    },
                    // Add capsule collider for the rigid body.
                    {
                        collider = ColliderBuilder::new(BaseBuilder::new())
                            .with_shape(ColliderShape::capsule_y(0.25, 0.2))
                            .build(&mut scene.graph);
                        collider
                    },
                ]),
        )
        // We don't want the player to tilt.
        .with_locked_rotations(true)
        // We don't want the rigid body to sleep (be excluded from simulation)
        .with_can_sleep(false)
        .build(&mut scene.graph);

        Self {
            camera,
            weapon_pivot,
            rigid_body: rigid_body_handle,
            controller: Default::default(),
            sender,
            collider,
            weapon: Default::default(), // Leave it unassigned for now.
        }
    }

    fn update(&mut self, scene: &mut Scene) {
        // Set pitch for the camera. These lines responsible for up-down camera rotation.
        scene.graph[self.camera].local_transform_mut().set_rotation(
            UnitQuaternion::from_axis_angle(&Vector3::x_axis(), self.controller.pitch.to_radians()),
        );

        // Borrow rigid body node.
        let body = scene.graph[self.rigid_body].as_rigid_body_mut();

        // Keep only vertical velocity, and drop horizontal.
        let mut velocity = Vector3::new(0.0, body.lin_vel().y, 0.0);

        // Change the velocity depending on the keys pressed.
        if self.controller.move_forward {
            // If we moving forward then add "look" vector of the body.
            velocity += body.look_vector();
        }
        if self.controller.move_backward {
            // If we moving backward then subtract "look" vector of the body.
            velocity -= body.look_vector();
        }
        if self.controller.move_left {
            // If we moving left then add "side" vector of the body.
            velocity += body.side_vector();
        }
        if self.controller.move_right {
            // If we moving right then subtract "side" vector of the body.
            velocity -= body.side_vector();
        }

        // Finally new linear velocity.
        body.set_lin_vel(velocity);

        // Change the rotation of the rigid body according to current yaw. These lines responsible for
        // left-right rotation.
        body.local_transform_mut()
            .set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::y_axis(),
                self.controller.yaw.to_radians(),
            ));

        if self.controller.shoot {
            self.sender
                .send(Message::ShootWeapon {
                    weapon: self.weapon,
                })
                .unwrap();
        }
    }

    fn process_input_event(&mut self, event: &Event<()>) {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::KeyboardInput { input, .. } => {
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
                &WindowEvent::MouseInput { button, state, .. } => {
                    if button == MouseButton::Left {
                        self.controller.shoot = state == ElementState::Pressed;
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { event, .. } => {
                if let DeviceEvent::MouseMotion { delta } = event {
                    let mouse_sens = 0.5;
                    self.controller.yaw -= mouse_sens * delta.0 as f32;

                    self.controller.pitch =
                        (self.controller.pitch + mouse_sens * delta.1 as f32).clamp(-90.0, 90.0);
                }
            }
            _ => (),
        }
    }
}

fn create_shot_trail(
    graph: &mut Graph,
    origin: Vector3<f32>,
    direction: Vector3<f32>,
    trail_length: f32,
) {
    let transform = TransformBuilder::new()
        .with_local_position(origin)
        // Scale the trail in XZ plane to make it thin, and apply `trail_length` scale on Y axis
        // to stretch is out.
        .with_local_scale(Vector3::new(0.0025, 0.0025, trail_length))
        // Rotate the trail along given `direction`
        .with_local_rotation(UnitQuaternion::face_towards(&direction, &Vector3::y()))
        .build();

    // Create unit cylinder with caps that faces toward Z axis.
    let shape = Arc::new(Mutex::new(SurfaceData::make_cylinder(
        6,     // Count of sides
        1.0,   // Radius
        1.0,   // Height
        false, // No caps are needed.
        // Rotate vertical cylinder around X axis to make it face towards Z axis
        &UnitQuaternion::from_axis_angle(&Vector3::x_axis(), 90.0f32.to_radians()).to_homogeneous(),
    )));

    // Create an instance of standard material for the shot trail.
    let mut material = Material::standard();
    material
        .set_property(
            &ImmutableString::new("diffuseColor"),
            // Set yellow-ish color.
            PropertyValue::Color(Color::from_rgba(255, 255, 0, 120)),
        )
        .unwrap();

    MeshBuilder::new(
        BaseBuilder::new()
            // Do not cast shadows.
            .with_cast_shadows(false)
            .with_local_transform(transform)
            // Shot trail should live ~0.25 seconds, after that it will be automatically
            // destroyed.
            .with_lifetime(0.25),
    )
    .with_surfaces(vec![SurfaceBuilder::new(shape)
        .with_material(Arc::new(Mutex::new(material)))
        .build()])
    // Make sure to set Forward render path, otherwise the object won't be
    // transparent.
    .with_render_path(RenderPath::Forward)
    .build(graph);
}

struct Game {
    scene: Handle<Scene>,
    player: Player,
    weapons: Pool<Weapon>,
    receiver: Receiver<Message>,
    sender: Sender<Message>,
}

impl Game {
    pub async fn new(engine: &mut Engine) -> Self {
        // Make message queue.
        let (sender, receiver) = mpsc::channel();

        let mut scene = Scene::new();

        // Load a scene resource and create its instance.
        engine
            .resource_manager
            .request_model("data/models/scene.rgs")
            .await
            .unwrap()
            .instantiate_geometry(&mut scene);

        // Create player first.
        let mut player =
            Player::new(&mut scene, engine.resource_manager.clone(), sender.clone()).await;

        // Create weapon next.
        let weapon = Weapon::new(&mut scene, engine.resource_manager.clone()).await;

        // "Attach" the weapon to the weapon pivot of the player.
        scene.graph.link_nodes(weapon.model(), player.weapon_pivot);

        // Create a container for the weapons.
        let mut weapons = Pool::new();

        // Put the weapon into it - this operation moves the weapon in the pool and returns handle.
        let weapon = weapons.spawn(weapon);

        // "Give" the weapon to the player.
        player.weapon = weapon;

        Self {
            player,
            scene: engine.scenes.add(scene),
            weapons,
            sender,
            receiver,
        }
    }

    fn shoot_weapon(&mut self, weapon: Handle<Weapon>, engine: &mut Engine) {
        let weapon = &mut self.weapons[weapon];

        if weapon.can_shoot() {
            weapon.shoot();

            let scene = &mut engine.scenes[self.scene];

            let weapon_model = &scene.graph[weapon.model()];

            // Make a ray that starts at the weapon's position in the world and look toward
            // "look" vector of the weapon.
            let ray = Ray::new(
                scene.graph[weapon.shot_point()].global_position(),
                weapon_model.look_vector().scale(1000.0),
            );

            let mut intersections = Vec::new();

            scene.graph.physics.cast_ray(
                RayCastOptions {
                    ray_origin: Point3::from(ray.origin),
                    max_len: ray.dir.norm(),
                    groups: Default::default(),
                    sort_results: true, // We need intersections to be sorted from closest to furthest.
                    ray_direction: ray.dir,
                },
                &mut intersections,
            );

            // Ignore intersections with player's capsule.
            let trail_length = if let Some(intersection) = intersections
                .iter()
                .find(|i| i.collider != self.player.collider)
            {
                //
                // TODO: Add code to handle intersections with bots.
                //

                // For now just apply some force at the point of impact.
                let colliders_parent = scene.graph[intersection.collider].parent();
                let picked_rigid_body = scene.graph[colliders_parent].as_rigid_body_mut();
                picked_rigid_body.apply_force_at_point(
                    ray.dir.normalize().scale(10.0),
                    intersection.position.coords,
                );
                picked_rigid_body.wake_up();

                // Add bullet impact effect.
                let effect_orientation = vector_to_quat(intersection.normal);

                create_bullet_impact(
                    &mut scene.graph,
                    engine.resource_manager.clone(),
                    intersection.position.coords,
                    effect_orientation,
                );

                // Trail length will be the length of line between intersection point and ray origin.
                (intersection.position.coords - ray.origin).norm()
            } else {
                // Otherwise trail length will be just the ray length.
                ray.dir.norm()
            };

            create_shot_trail(&mut scene.graph, ray.origin, ray.dir, trail_length);
        }
    }

    pub fn update(&mut self, engine: &mut Engine, dt: f32) {
        let scene = &mut engine.scenes[self.scene];

        self.player.update(scene);

        for weapon in self.weapons.iter_mut() {
            weapon.update(dt, &mut scene.graph);
        }

        // We're using `try_recv` here because we don't want to wait until next message -
        // if the queue is empty just continue to next frame.
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::ShootWeapon { weapon } => {
                    self.shoot_weapon(weapon, engine);
                }
            }
        }
    }
}

fn main() {
    // Configure main window first.
    let window_builder = WindowBuilder::new().with_title("3D Shooter Tutorial");
    // Create event loop that will be used to "listen" events from the OS.
    let event_loop = EventLoop::new();

    // Finally create an instance of the engine.
    let serialization_context = Arc::new(SerializationContext::new());
    let mut engine = Engine::new(EngineInitParams {
        window_builder,
        resource_manager: ResourceManager::new(serialization_context.clone()),
        serialization_context,
        events_loop: &event_loop,
        vsync: false,
    })
    .unwrap();

    // Initialize game instance.
    let mut game = fyrox::core::futures::executor::block_on(Game::new(&mut engine));

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
                    game.update(&mut engine, TIMESTEP);

                    // Update engine each frame.
                    engine.update(TIMESTEP, control_flow);
                }

                // Rendering must be explicitly requested and handled after RedrawRequested event is received.
                engine.get_window().request_redraw();
            }
            Event::RedrawRequested(_) => {
                // Render at max speed - it is not tied to the game code.
                engine.render().unwrap();
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
                    engine.set_frame_size(size.into()).unwrap();
                }
                _ => (),
            },
            _ => *control_flow = ControlFlow::Poll,
        }
    });
}
