use crate::weapon::Weapon;
use rg3d::core::pool::Handle;

pub enum Message {
    ShootWeapon { weapon: Handle<Weapon> },
}
