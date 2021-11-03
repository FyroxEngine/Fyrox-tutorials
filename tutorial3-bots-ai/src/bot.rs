use rg3d::engine::resource_manager::MaterialSearchOptions;
use rg3d::{
    animation::{
        machine::{Machine, Parameter, PoseNode, State, Transition},
        Animation,
    },
    core::{
        algebra::{UnitQuaternion, Vector3},
        pool::Handle,
    },
    engine::resource_manager::ResourceManager,
    physics3d::{
        rapier::dynamics::RigidBodyBuilder, rapier::geometry::ColliderBuilder, ColliderHandle,
        RigidBodyHandle,
    },
    resource::model::Model,
    scene::{base::BaseBuilder, node::Node, Scene},
};

pub struct Bot {
    pivot: Handle<Node>,
    rigid_body: RigidBodyHandle,
    collider: ColliderHandle,
    machine: BotAnimationMachine,
    follow_target: bool,
}

impl Bot {
    pub async fn new(
        scene: &mut Scene,
        position: Vector3<f32>,
        resource_manager: ResourceManager,
    ) -> Self {
        // Load bot 3D model as usual.
        let model = resource_manager
            .request_model("data/models/zombie.fbx", MaterialSearchOptions::RecursiveUp)
            .await
            .unwrap()
            .instantiate_geometry(scene);

        scene.graph[model]
            .local_transform_mut()
            // Move the model a bit down to make sure bot's feet will be on ground.
            .set_position(Vector3::new(0.0, -0.45, 0.0))
            // Scale the model because it is too big.
            .set_scale(Vector3::new(0.0047, 0.0047, 0.0047));

        let pivot = BaseBuilder::new()
            .with_children(&[model])
            .build(&mut scene.graph);

        // Create rigid body, it will be used for interaction with the world.
        let rigid_body = scene.physics.add_body(
            RigidBodyBuilder::new_dynamic()
                .lock_rotations() // We don't want a bot to tilt.
                .translation(Vector3::new(position.x, position.y, position.z)) // Set desired position.
                .build(),
        );

        // Add capsule collider for the rigid body.
        let collider = scene
            .physics
            .add_collider(ColliderBuilder::capsule_y(0.25, 0.2).build(), &rigid_body);

        // Bind pivot with rigid body. Scene will automatically sync transform of the pivot
        // with the transform of the rigid body.
        scene.physics_binder.bind(pivot, rigid_body);

        Self {
            machine: BotAnimationMachine::new(scene, model, resource_manager).await,
            pivot,
            rigid_body,
            collider,
            follow_target: false,
        }
    }

    pub fn update(&mut self, scene: &mut Scene, dt: f32, target: Vector3<f32>) {
        let attack_distance = 0.6;

        // Simple AI - follow target by a straight line.
        let self_position = scene.graph[self.pivot].global_position();
        let direction = target - self_position;

        // Distance to target.
        let distance = direction.norm();

        if distance != 0.0 && distance < 1.5 {
            self.follow_target = true;
        }

        if self.follow_target && distance != 0.0 {
            let rigid_body = scene.physics.bodies.get_mut(&self.rigid_body).unwrap();

            // Make sure bot is facing towards the target.
            let mut position = *rigid_body.position();
            position.rotation = UnitQuaternion::face_towards(
                &Vector3::new(direction.x, 0.0, direction.z),
                &Vector3::y_axis(),
            );
            rigid_body.set_position(position, true);

            // Move only if we're far enough from the target.
            if distance > attack_distance {
                // Normalize direction vector and scale it by movement speed.
                let xz_velocity = direction.scale(1.0 / distance).scale(0.9);

                let new_velocity =
                    Vector3::new(xz_velocity.x, rigid_body.linvel().y, xz_velocity.z);

                rigid_body.set_linvel(new_velocity, true);
            }
        }

        // For now these are set to false which will force bot to be in idle state.
        let input = BotAnimationMachineInput {
            walk: self.follow_target && distance > attack_distance,
            attack: distance < attack_distance,
        };

        self.machine.update(scene, dt, input);
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

pub struct BotAnimationMachineInput {
    // Whether a bot is walking or not.
    pub walk: bool,
    // Whether a bot is attacking or not.
    pub attack: bool,
}

pub struct BotAnimationMachine {
    machine: Machine,
}

impl BotAnimationMachine {
    // Names of parameters that will be used for transition rules in machine.
    const IDLE_TO_WALK: &'static str = "IdleToWalk";
    const WALK_TO_IDLE: &'static str = "WalkToIdle";
    const WALK_TO_ATTACK: &'static str = "WalkToAttack";
    const IDLE_TO_ATTACK: &'static str = "IdleToAttack";
    const ATTACK_TO_IDLE: &'static str = "AttackToIdle";
    const ATTACK_TO_WALK: &'static str = "AttackToWalk";

    pub async fn new(
        scene: &mut Scene,
        model: Handle<Node>,
        resource_manager: ResourceManager,
    ) -> Self {
        let mut machine = Machine::new();

        // Load animations in parallel.
        let (walk_animation_resource, idle_animation_resource, attack_animation_resource) = rg3d::core::futures::join!(
            resource_manager.request_model(
                "data/animations/zombie_walk.fbx",
                MaterialSearchOptions::RecursiveUp
            ),
            resource_manager.request_model(
                "data/animations/zombie_idle.fbx",
                MaterialSearchOptions::RecursiveUp
            ),
            resource_manager.request_model(
                "data/animations/zombie_attack.fbx",
                MaterialSearchOptions::RecursiveUp
            ),
        );

        // Now create three states with different animations.
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

        let (attack_animation, attack_state) = create_play_animation_state(
            attack_animation_resource.unwrap(),
            "Attack",
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
        machine.add_transition(Transition::new(
            "Walk->Attack",
            walk_state,
            attack_state,
            0.4,
            Self::WALK_TO_ATTACK,
        ));
        machine.add_transition(Transition::new(
            "Idle->Attack",
            idle_state,
            attack_state,
            0.4,
            Self::IDLE_TO_ATTACK,
        ));
        machine.add_transition(Transition::new(
            "Attack->Idle",
            attack_state,
            idle_state,
            0.4,
            Self::ATTACK_TO_IDLE,
        ));
        machine.add_transition(Transition::new(
            "Attack->Walk",
            attack_state,
            walk_state,
            0.4,
            Self::ATTACK_TO_WALK,
        ));

        // Define entry state.
        machine.set_entry_state(idle_state);

        Self { machine }
    }

    pub fn update(&mut self, scene: &mut Scene, dt: f32, input: BotAnimationMachineInput) {
        self.machine
            // Set transition parameters.
            .set_parameter(Self::WALK_TO_IDLE, Parameter::Rule(!input.walk))
            .set_parameter(Self::IDLE_TO_WALK, Parameter::Rule(input.walk))
            .set_parameter(Self::WALK_TO_ATTACK, Parameter::Rule(input.attack))
            .set_parameter(Self::IDLE_TO_ATTACK, Parameter::Rule(input.attack))
            .set_parameter(Self::ATTACK_TO_IDLE, Parameter::Rule(!input.attack))
            .set_parameter(Self::ATTACK_TO_WALK, Parameter::Rule(!input.attack))
            // Update machine and evaluate final pose.
            .evaluate_pose(&scene.animations, dt)
            // Apply the pose to the graph.
            .apply(&mut scene.graph);
    }
}
