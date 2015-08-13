/// Reinventing the famous crate `byteorder`

use std::mem::transmute;
use std::fmt;

pub type Result<T> = ::std::result::Result<T, Error>;
pub enum Error { }
impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

pub trait ReadBytesExt {
    fn read_u8(&self) -> Result<u8>;
    fn read_le_u16(&self) -> Result<u16>;
    fn read_le_u32(&self) -> Result<u32>;
}

pub trait WriteBytesExt {
    fn write_u8(&mut self, u8) -> Result<()>;
    fn write_le_u16(&mut self, u16) -> Result<()>;
    fn write_le_u32(&mut self, u32) -> Result<()>;
}

impl<'a> ReadBytesExt for &'a [u8] {
    fn read_u8(&self) -> Result<u8> {
        Ok(self[0])
    }
    fn read_le_u16(&self) -> Result<u16> {
        let mut n: u16 = 0;
        n = self[0] as u16;
        n |= (self[1] as u16) << 8;
        Ok(n)
    }
    fn read_le_u32(&self) -> Result<u32> {
        let mut n: u32 = 0;
        n = self[0] as u32;
        n |= (self[1] as u32) << 8;
        n |= (self[2] as u32) << 16;
        n |= (self[3] as u32) << 24;
        Ok(n)
    }
}

impl<'a> WriteBytesExt for &'a mut [u8] {
    fn write_u8(&mut self, n: u8) -> Result<()> {
        self[0] = n;
        Ok(())
    }
    fn write_le_u16(&mut self, n: u16) -> Result<()> {
        self[0] = (n & 0xFF) as u8;
        self[1] = ((n >> 8) & 0xFF) as u8;

        Ok(())
    }
    fn write_le_u32(&mut self, n: u32) -> Result<()> {
        self[0] = (n & 0xFF) as u8;
        self[1] = ((n >> 8) & 0xFF) as u8;
        self[2] = ((n >> 16) & 0xFF) as u8;
        self[3] = ((n >> 24) & 0xFF) as u8;
        Ok(())
    }
}
