use crate::weapon::Weapon;
use fyrox::core::pool::Handle;

pub enum Message {
    ShootWeapon { weapon: Handle<Weapon> },
}
