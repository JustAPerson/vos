use std::path::Path;

use disk::Result;

pub mod fat;
pub use self::fat::Fat32;

pub trait FileSystem {
    // TODO: consider rewriting FileSystem::write_file() accepting T: Read
    fn write_file(&mut self, &Path, &[u8]) -> Result<()>;
    fn read_file(&mut self, &Path, &mut [u8]) -> Result<()>;
    fn delete(&mut self, &Path) -> Result<()>;
    fn make_dir(&mut self, &Path) -> Result<()>;
}

