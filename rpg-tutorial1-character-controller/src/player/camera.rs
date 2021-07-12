use rg3d::core::algebra::UnitQuaternion;
use rg3d::event::DeviceEvent;
use rg3d::{
    core::{algebra::Vector3, pool::Handle},
    engine::resource_manager::ResourceManager,
    resource::texture::TextureWrapMode,
    scene::{
        base::BaseBuilder,
        camera::{CameraBuilder, SkyBox, SkyBoxBuilder},
        graph::Graph,
        node::Node,
        transform::TransformBuilder,
    },
};

// Camera controller consists of three scene nodes - two pivots and one camera.
pub struct CameraController {
    // Pivot is the origin of our camera controller.
    pub pivot: Handle<Node>,
    // Hinge node is used to rotate the camera around X axis with some spacing.
    hinge: Handle<Node>,
    // Camera is our eyes in the world.
    camera: Handle<Node>,
    // An angle around local Y axis of the pivot.
    pub yaw: f32,
    // An angle around local X axis of the hinge.
    pitch: f32,
}

impl CameraController {
    pub async fn new(graph: &mut Graph, resource_manager: ResourceManager) -> Self {
        let camera;
        let hinge;
        let pivot = BaseBuilder::new()
            .with_children(&[{
                hinge = BaseBuilder::new()
                    .with_local_transform(
                        TransformBuilder::new()
                            .with_local_position(Vector3::new(0.0, 0.55, 0.0))
                            .build(),
                    )
                    .with_children(&[{
                        camera = CameraBuilder::new(
                            BaseBuilder::new().with_local_transform(
                                TransformBuilder::new()
                                    .with_local_position(Vector3::new(0.0, 0.0, -2.0))
                                    .build(),
                            ),
                        )
                        .with_skybox(create_skybox(resource_manager).await)
                        .build(graph);
                        camera
                    }])
                    .build(graph);
                hinge
            }])
            .build(graph);

        Self {
            pivot,
            hinge,
            camera,
            yaw: 0.0,
            pitch: 0.0,
        }
    }

    pub fn handle_device_event(&mut self, device_event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = device_event {
            const MOUSE_SENSITIVITY: f32 = 0.015;

            self.yaw -= (delta.0 as f32) * MOUSE_SENSITIVITY;
            self.pitch = (self.pitch + (delta.1 as f32) * MOUSE_SENSITIVITY)
                // Limit vertical angle to [-90; 90] degrees range
                .max(-90.0f32.to_radians())
                .min(90.0f32.to_radians());
        }
    }

    pub fn update(&mut self, graph: &mut Graph) {
        // Apply rotation to the pivot.
        graph[self.pivot]
            .local_transform_mut()
            .set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::y_axis(),
                self.yaw,
            ));

        // Apply rotation to the hinge.
        graph[self.hinge]
            .local_transform_mut()
            .set_rotation(UnitQuaternion::from_axis_angle(
                &Vector3::x_axis(),
                self.pitch,
            ));
    }
}

// Creates a new sky box, this code was taken from "Writing a 3D shooter using rg3d" tutorial
// series.
async fn create_skybox(resource_manager: ResourceManager) -> SkyBox {
    // Load skybox textures in parallel.
    let (front, back, left, right, top, bottom) = rg3d::core::futures::join!(
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
    let cubemap = skybox.cubemap();
    let mut data = cubemap.as_ref().unwrap().data_ref();
    data.set_s_wrap_mode(TextureWrapMode::ClampToEdge);
    data.set_t_wrap_mode(TextureWrapMode::ClampToEdge);

    skybox
}
