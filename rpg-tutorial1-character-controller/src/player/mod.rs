use crate::player::camera::CameraController;
use fyrox::{
    animation::{
        machine::{Machine, Parameter, PoseNode, State, Transition},
        Animation,
    },
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    engine::resource_manager::ResourceManager,
    event::{DeviceEvent, ElementState, KeyboardInput, VirtualKeyCode},
    resource::model::Model,
    scene::{
        base::BaseBuilder, collider::ColliderBuilder, collider::ColliderShape,
        graph::physics::CoefficientCombineRule, node::Node, rigidbody::RigidBodyBuilder,
        transform::TransformBuilder, Scene,
    },
};

mod camera;

pub struct Player {
    model: Handle<Node>,
    camera_controller: CameraController,
    input_controller: InputController,
    body: Handle<Node>,
    collider: Handle<Node>,
    animation_machine: AnimationMachine,
}

#[derive(Default)]
struct InputController {
    walk_forward: bool,
    walk_backward: bool,
    walk_left: bool,
    walk_right: bool,
}

impl Player {
    pub async fn new(resource_manager: ResourceManager, scene: &mut Scene) -> Self {
        // Load paladin 3D model and create its instance in the scene.
        let model = resource_manager
            .request_model("data/models/paladin/paladin.fbx")
            .await
            .unwrap()
            .instantiate_geometry(scene);

        // Scale down paladin's model because it is too big.
        scene.graph[model]
            .local_transform_mut()
            .set_position(Vector3::new(0.0, -0.75, 0.0))
            .set_scale(Vector3::new(0.02, 0.02, 0.02));

        // Create new rigid body and offset it a bit to prevent falling through the ground.
        let collider;
        let body = RigidBodyBuilder::new(
            BaseBuilder::new()
                .with_local_transform(
                    TransformBuilder::new()
                        .with_local_position(Vector3::new(0.0, 2.0, 0.0))
                        .build(),
                )
                .with_children(&[
                    {
                        // Attach the model to the pivot. This will force model to move together with the pivot.
                        model
                    },
                    {
                        // Create capsule collider with friction disabled. We need to disable friction because linear
                        // velocity will be set manually, but the physics engine will reduce it using friction so it
                        // won't let us to set linear velocity precisely.
                        collider = ColliderBuilder::new(BaseBuilder::new())
                            .with_shape(ColliderShape::capsule_y(0.55, 0.15))
                            .with_friction_combine_rule(CoefficientCombineRule::Min)
                            .with_friction(0.0)
                            .build(&mut scene.graph);
                        collider
                    },
                ]),
        )
        .with_locked_rotations(true)
        .with_can_sleep(false)
        .build(&mut scene.graph);

        Self {
            model,

            animation_machine: AnimationMachine::new(scene, model, resource_manager.clone()).await,

            // As a final stage create camera controller.
            camera_controller: CameraController::new(&mut scene.graph, resource_manager).await,

            input_controller: Default::default(),
            collider,
            body,
        }
    }

    pub fn handle_device_event(&mut self, device_event: &DeviceEvent) {
        self.camera_controller.handle_device_event(device_event)
    }

    pub fn handle_key_event(&mut self, key: &KeyboardInput) {
        if let Some(key_code) = key.virtual_keycode {
            match key_code {
                VirtualKeyCode::W => {
                    self.input_controller.walk_forward = key.state == ElementState::Pressed
                }
                VirtualKeyCode::S => {
                    self.input_controller.walk_backward = key.state == ElementState::Pressed
                }
                VirtualKeyCode::A => {
                    self.input_controller.walk_left = key.state == ElementState::Pressed
                }
                VirtualKeyCode::D => {
                    self.input_controller.walk_right = key.state == ElementState::Pressed
                }
                _ => (),
            }
        }
    }

    pub fn update(&mut self, scene: &mut Scene, dt: f32) {
        self.camera_controller.update(&mut scene.graph);

        let body = scene.graph[self.body].as_rigid_body_mut();

        let look_vector = body
            .look_vector()
            .try_normalize(f32::EPSILON)
            .unwrap_or(Vector3::z());

        let side_vector = body
            .side_vector()
            .try_normalize(f32::EPSILON)
            .unwrap_or(Vector3::x());

        let position = **body.local_transform().position();

        let mut velocity = Vector3::default();

        if self.input_controller.walk_right {
            velocity -= side_vector;
        }
        if self.input_controller.walk_left {
            velocity += side_vector;
        }
        if self.input_controller.walk_forward {
            velocity += look_vector;
        }
        if self.input_controller.walk_backward {
            velocity -= look_vector;
        }

        let speed = 1.35 * dt;
        let velocity = velocity
            .try_normalize(f32::EPSILON)
            .and_then(|v| Some(v.scale(speed)))
            .unwrap_or(Vector3::default());

        // Apply linear velocity.
        body.set_lin_vel(Vector3::new(
            velocity.x / dt,
            body.lin_vel().y,
            velocity.z / dt,
        ));

        let is_moving = velocity.norm_squared() > 0.0;
        if is_moving {
            // Since we have free camera while not moving, we have to sync rotation of pivot
            // with rotation of camera so character will start moving in look direction.
            body.local_transform_mut()
                .set_rotation(UnitQuaternion::from_axis_angle(
                    &Vector3::y_axis(),
                    self.camera_controller.yaw,
                ));

            // Apply additional rotation to model - it will turn in front of walking direction.
            let angle: f32 = if self.input_controller.walk_left {
                if self.input_controller.walk_forward {
                    45.0
                } else if self.input_controller.walk_backward {
                    135.0
                } else {
                    90.0
                }
            } else if self.input_controller.walk_right {
                if self.input_controller.walk_forward {
                    -45.0
                } else if self.input_controller.walk_backward {
                    -135.0
                } else {
                    -90.0
                }
            } else if self.input_controller.walk_backward {
                180.0
            } else {
                0.0
            };

            scene.graph[self.model].local_transform_mut().set_rotation(
                UnitQuaternion::from_axis_angle(&Vector3::y_axis(), angle.to_radians()),
            );
        }

        // Sync camera controller position with player's position.
        scene.graph[self.camera_controller.pivot]
            .local_transform_mut()
            .set_position(position + velocity);

        self.animation_machine
            .update(scene, dt, AnimationMachineInput { walk: is_moving });
    }
}

// Simple helper method to create a state supplied with PlayAnimation node.
fn create_play_animation_state(
    animation_resource: Model,
    name: &str,
    machine: &mut Machine,
    scene: &mut Scene,
    model: Handle<Node>,
) -> (Handle<Animation>, Handle<State>) {
    // Animations retargetting just makes an instance of animation and binds it to
    // given model using names of bones.
    let animation = *animation_resource
        .retarget_animations(model, scene)
        .get(0)
        .unwrap();
    // Create new PlayAnimation node and add it to machine.
    let node = machine.add_node(PoseNode::make_play_animation(animation));
    // Make a state using the node we've made.
    let state = machine.add_state(State::new(name, node));
    (animation, state)
}

pub struct AnimationMachineInput {
    // Whether a bot is walking or not.
    pub walk: bool,
}

pub struct AnimationMachine {
    machine: Machine,
}

impl AnimationMachine {
    // Names of parameters that will be used for transition rules in machine.
    const IDLE_TO_WALK: &'static str = "IdleToWalk";
    const WALK_TO_IDLE: &'static str = "WalkToIdle";

    pub async fn new(
        scene: &mut Scene,
        model: Handle<Node>,
        resource_manager: ResourceManager,
    ) -> Self {
        let mut machine = Machine::new(model);

        // Load animations in parallel.
        let (walk_animation_resource, idle_animation_resource) = fyrox::core::futures::join!(
            resource_manager.request_model("data/models/paladin/walk.fbx"),
            resource_manager.request_model("data/models/paladin/idle.fbx"),
        );

        // Now create two states with different animations.
        let (_, idle_state) = create_play_animation_state(
            idle_animation_resource.unwrap(),
            "Idle",
            &mut machine,
            scene,
            model,
        );

        let (walk_animation, walk_state) = create_play_animation_state(
            walk_animation_resource.unwrap(),
            "Walk",
            &mut machine,
            scene,
            model,
        );

        // Next, define transitions between states.
        machine.add_transition(Transition::new(
            // A name for debugging.
            "Idle->Walk",
            // Source state.
            idle_state,
            // Target state.
            walk_state,
            // Transition time in seconds.
            0.4,
            // A name of transition rule parameter.
            Self::IDLE_TO_WALK,
        ));
        machine.add_transition(Transition::new(
            "Walk->Idle",
            walk_state,
            idle_state,
            0.4,
            Self::WALK_TO_IDLE,
        ));

        // Define entry state.
        machine.set_entry_state(idle_state);

        Self { machine }
    }

    pub fn update(&mut self, scene: &mut Scene, dt: f32, input: AnimationMachineInput) {
        self.machine
            // Set transition parameters.
            .set_parameter(Self::WALK_TO_IDLE, Parameter::Rule(!input.walk))
            .set_parameter(Self::IDLE_TO_WALK, Parameter::Rule(input.walk))
            // Update machine and evaluate final pose.
            .evaluate_pose(&scene.animations, dt)
            // Apply the pose to the graph.
            .apply(&mut scene.graph);
    }
}
