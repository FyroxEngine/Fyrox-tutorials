use crate::{bot::Bot, message::Message, weapon::Weapon};
use rg3d::core::algebra::Point3;
use rg3d::core::parking_lot::Mutex;
use rg3d::core::sstorage::ImmutableString;
use rg3d::engine::resource_manager::MaterialSearchOptions;
use rg3d::material::{Material, PropertyValue};
use rg3d::scene::camera::SkyBoxBuilder;
use rg3d::{
    core::{
        algebra::{UnitQuaternion, Vector3},
        color::Color,
        color_gradient::{ColorGradient, GradientPoint},
        math::ray::Ray,
        pool::{Handle, Pool},
    },
    engine::{resource_manager::ResourceManager, Engine},
    event::{DeviceEvent, ElementState, Event, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    physics3d::{
        rapier::dynamics::RigidBodyBuilder, rapier::geometry::ColliderBuilder, ColliderHandle,
        RayCastOptions, RigidBodyHandle,
    },
    resource::texture::TextureWrapMode,
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, SkyBox},
        graph::Graph,
        mesh::{
            surface::{SurfaceBuilder, SurfaceData},
            MeshBuilder, RenderPath,
        },
        node::Node,
        particle_system::{
            emitter::base::BaseEmitterBuilder, emitter::sphere::SphereEmitterBuilder,
            ParticleSystemBuilder,
        },
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

pub mod bot;
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
    pivot: Handle<Node>,
    camera: Handle<Node>,
    rigid_body: RigidBodyHandle,
    controller: InputController,
    weapon_pivot: Handle<Node>,
    sender: Sender<Message>,
    weapon: Handle<Weapon>,
    collider: ColliderHandle,
}

async fn create_skybox(resource_manager: ResourceManager) -> SkyBox {
    // Load skybox textures in parallel.
    let (front, back, left, right, top, bottom) = rg3d::core::futures::join!(
        resource_manager.request_texture("data/textures/skybox/front.jpg", None),
        resource_manager.request_texture("data/textures/skybox/back.jpg", None),
        resource_manager.request_texture("data/textures/skybox/left.jpg", None),
        resource_manager.request_texture("data/textures/skybox/right.jpg", None),
        resource_manager.request_texture("data/textures/skybox/up.jpg", None),
        resource_manager.request_texture("data/textures/skybox/down.jpg", None)
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
            .with_spawn_rate(1000)
            .with_size_modifier_range(-0.01..-0.0125)
            .with_size_range(0.0010..0.025)
            .with_x_velocity_range(-0.01..0.01)
            .with_y_velocity_range(0.017..0.02)
            .with_z_velocity_range(-0.01..0.01)
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
    .with_acceleration(Vector3::new(0.0, -10.0, 0.0))
    .with_color_over_lifetime_gradient(color_gradient)
    .with_emitters(vec![emitter])
    // We'll use simple spark texture for each particle.
    .with_texture(resource_manager.request_texture(Path::new("data/textures/spark.png"), None))
    .build(graph)
}

impl Player {
    async fn new(
        scene: &mut Scene,
        resource_manager: ResourceManager,
        sender: Sender<Message>,
    ) -> Self {
        // Create a pivot and attach a camera to it, move it a bit up to "emulate" head.
        let camera;
        let weapon_pivot;
        let pivot = BaseBuilder::new()
            .with_children(&[{
                camera = CameraBuilder::new(
                    BaseBuilder::new()
                        .with_local_transform(
                            TransformBuilder::new()
                                .with_local_position(Vector3::new(0.0, 0.25, 0.0))
                                .build(),
                        )
                        .with_children(&[{
                            weapon_pivot = BaseBuilder::new()
                                .with_local_transform(
                                    TransformBuilder::new()
                                        .with_local_position(Vector3::new(-0.1, -0.05, 0.015))
                                        .build(),
                                )
                                .build(&mut scene.graph);
                            weapon_pivot
                        }]),
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
                .translation(Vector3::new(0.0, 1.0, -1.0)) // Offset player a bit.
                .build(),
        );

        // Add capsule collider for the rigid body.
        let collider = scene.physics.add_collider(
            ColliderBuilder::capsule_y(0.25, 0.2).build(),
            &rigid_body_handle,
        );

        // Bind pivot with rigid body. Scene will automatically sync transform of the pivot
        // with the transform of the rigid body.
        scene.physics_binder.bind(pivot, rigid_body_handle);

        Self {
            pivot,
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

        // Borrow the pivot in the graph.
        let pivot = &mut scene.graph[self.pivot];

        // Borrow rigid body in the physics.
        let body = scene.physics.bodies.get_mut(&self.rigid_body).unwrap();

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
            .with_local_transform(transform)
            // Shot trail should live ~0.25 seconds, after that it will be automatically
            // destroyed.
            .with_lifetime(0.25),
    )
    .with_surfaces(vec![SurfaceBuilder::new(shape)
        .with_material(Arc::new(Mutex::new(material)))
        .build()])
    // Do not cast shadows.
    .with_cast_shadows(false)
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
    bots: Pool<Bot>,
}

impl Game {
    pub async fn new(engine: &mut Engine) -> Self {
        // Make message queue.
        let (sender, receiver) = mpsc::channel();

        let mut scene = Scene::new();

        // Load a scene resource and create its instance.
        engine
            .resource_manager
            .request_model(
                "data/models/scene.rgs",
                MaterialSearchOptions::UsePathDirectly,
            )
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

        // Add some bots.
        let mut bots = Pool::new();

        bots.spawn(
            Bot::new(
                &mut scene,
                Vector3::new(-1.0, 1.0, 1.5),
                engine.resource_manager.clone(),
            )
            .await,
        );

        Self {
            player,
            scene: engine.scenes.add(scene),
            weapons,
            sender,
            receiver,
            bots,
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

            scene.physics.cast_ray(
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
                let colliders_parent = scene
                    .physics
                    .colliders
                    .get(&intersection.collider)
                    .unwrap()
                    .parent()
                    .unwrap();
                scene
                    .physics
                    .bodies
                    .native_mut(colliders_parent)
                    .unwrap()
                    .apply_force_at_point(
                        ray.dir.normalize().scale(10.0),
                        intersection.position,
                        true,
                    );

                // Add bullet impact effect.
                let effect_orientation = if intersection.normal.normalize() == Vector3::y() {
                    // Handle singularity when normal of impact point is collinear with Y axis.
                    UnitQuaternion::from_axis_angle(&Vector3::y_axis(), 0.0)
                } else {
                    UnitQuaternion::face_towards(&intersection.normal, &Vector3::y())
                };

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

        let target = scene.graph[self.player.pivot].global_position();

        for bot in self.bots.iter_mut() {
            bot.update(scene, dt, target);
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
    let mut engine = Engine::new(window_builder, &event_loop, true).unwrap();

    // Initialize game instance.
    let mut game = rg3d::core::futures::executor::block_on(Game::new(&mut engine));

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
                    engine.update(TIMESTEP);
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
