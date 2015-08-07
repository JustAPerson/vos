use std::ops::{Deref, DerefMut};
use std::clone::Clone;
use std::marker::Copy;

pub struct Sector(pub [u8; 512]);
pub static EMPTY_SECTOR: Sector = Sector([0; 512]);

impl Clone for Sector {
    fn clone(&self) -> Sector {
        let buf = self.0;
        Sector(buf)
    }
}
impl Copy for Sector { }

impl Deref for Sector {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
impl DerefMut for Sector {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

