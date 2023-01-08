use fyrox::{
    core::pool::Handle,
    engine::resource_manager::ResourceManager,
    scene::{node::Node, Scene},
};

pub struct Level {
    root: Handle<Node>,
}

impl Level {
    pub async fn new(resource_manager: ResourceManager, scene: &mut Scene) -> Self {
        let root = resource_manager
            .request_model("data/levels/level.rgs")
            .await
            .unwrap()
            .instantiate(scene);

        Self { root }
    }
}
