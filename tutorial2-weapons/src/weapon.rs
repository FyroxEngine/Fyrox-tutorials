use fyrox::scene::graph::Graph;
use fyrox::{
    core::{algebra::Vector3, math::Vector3Ext, pool::Handle},
    engine::resource_manager::ResourceManager,
    scene::{node::Node, Scene},
};

pub struct Weapon {
    model: Handle<Node>,
    shot_point: Handle<Node>,
    shot_timer: f32,
    recoil_offset: Vector3<f32>,
    recoil_target_offset: Vector3<f32>,
}

impl Weapon {
    pub async fn new(scene: &mut Scene, resource_manager: ResourceManager) -> Self {
        // Yeah, you need only few lines of code to load a model of any complexity.
        let model = resource_manager
            .request_model("data/models/m4.FBX")
            .await
            .unwrap()
            .instantiate(scene);

        let shot_point = scene.graph.find_by_name(model, "Weapon:ShotPoint");

        Self {
            model,
            shot_point,
            shot_timer: 0.0,
            recoil_offset: Default::default(),
            recoil_target_offset: Default::default(),
        }
    }

    pub fn model(&self) -> Handle<Node> {
        self.model
    }

    pub fn shot_point(&self) -> Handle<Node> {
        self.shot_point
    }

    pub fn update(&mut self, dt: f32, graph: &mut Graph) {
        self.shot_timer = (self.shot_timer - dt).max(0.0);

        // `follow` method defined in Vector3Ext trait and it just increases or
        // decreases vector's value in order to "follow" the target value with
        // given speed.
        self.recoil_offset.follow(&self.recoil_target_offset, 0.5);

        // Apply offset to weapon's model.
        graph[self.model]
            .local_transform_mut()
            .set_position(self.recoil_offset);

        // Check if we've reached target recoil offset.
        if self
            .recoil_offset
            .metric_distance(&self.recoil_target_offset)
            < 0.001
        {
            // And if so, reset offset to zero to return weapon at
            // its default position.
            self.recoil_target_offset = Default::default();
        }
    }

    pub fn can_shoot(&self) -> bool {
        self.shot_timer <= 0.0
    }

    pub fn shoot(&mut self) {
        self.shot_timer = 0.1;

        self.recoil_target_offset = Vector3::new(0.0, 0.0, -0.025);
    }
}
