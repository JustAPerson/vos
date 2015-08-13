use std::path::PathBuf;

pub mod sector;
pub use disk::sector::{Sector, EMPTY_SECTOR};

#[cfg(not(feature="bootdriver"))]
pub mod ramdisk;
#[cfg(not(feature="bootdriver"))]
pub use disk::ramdisk::RamDisk;

mod biosdisk;
pub use self::biosdisk::BiosDisk;

use byteorder::{ReadBytesExt, WriteBytesExt};
use fs::{Fat32, FileSystem};

pub type Result<T> = ::std::result::Result<T, Error>;
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    BeyondDiskSize,
    CorruptDisk,
    CorruptFAT,
    WriteError,
    InvalidPath,
    Nonexistent(PathBuf)
}

pub trait Disk {
    fn info(&self) -> DiskInfo;
    fn read_sector(&self, lba: usize) -> Result<&Sector>;
    fn write_sector(&mut self, lba: usize, data: &[u8]) -> Result<()>;
}

pub enum Format {
    Fat32,
    Unrecognized(u8),
}

impl Format {
    pub fn serialize(&self) -> u8 {
        use self::Format::*;
        match *self {
           Fat32 => 0x0C,
           Unrecognized(n) => n,
        }
    }
}

pub struct DiskInfo {
    pub size: usize,
    pub sector_size: usize,
}

pub struct Partition {
    device: *mut Disk,
    start: usize,
    size: usize,
}

pub fn mount<T: Disk + 'static>(disk: &mut T, index: usize) -> Result<Box<FileSystem>> {
    let pinfo = match try!(get_pinfo(disk, index)) {
        Some(pinfo) => pinfo,
        None => panic!("Cannot mount invalid partition: index={}", index),
    };

    // TODO: any checks necessary here?
    let partition = Partition {
        device: disk as *mut Disk,
        start: pinfo.start,
        size: pinfo.size,
    };

    match pinfo.format {
        Format::Fat32 => Ok(Box::new(try!(Fat32::new(Box::new(partition))))),
        Format::Unrecognized(n) => panic!("Cannot mount unrecognized partition: {:x}", n),
    }
}

pub fn get_partition<T: Disk + 'static>(disk: &mut T, index: usize) -> Result<Partition> {
    let pinfo = match try!(get_pinfo(disk, index)) {
        Some(pinfo) => pinfo,
        None => panic!("Cannot mount invalid partition: index={}", index),
    };

    // TODO: any checks necessary here?
    let partition = Partition {
        device: disk as *mut Disk,
        start: pinfo.start,
        size: pinfo.size,
    };

    Ok(partition)
}

pub struct PartitionInfo {
    pub format: Format,
    pub start: usize,
    pub size: usize,
    pub bootable: bool,
}

pub fn get_pinfo<T: Disk>(disk: &T, index: usize) -> Result<Option<PartitionInfo>> {
    // read first sector to find MBR
    let mbr = try!(disk.read_sector(0));

    // ensure valid MBR
    if mbr[510..512] != [0x55, 0xAA] {
        return Err(Error::CorruptDisk)
    }

    let pt = &mbr[446..509]; // the 4 * 16 byte partition table
    assert!(index < 4, "Invalid partion table index: {}", index);
    let entry = &pt[index * 16 .. (index + 1) * 16];

    let format = match entry[4] {
        // There are several FAT32 types
        // 0x0B is for FAT32 with CHS addressing
        // Currently only support 0x0C for FAT32 with LBA addressing
        0x0C => Format::Fat32,

        // Unused partition table entry. Return no info
        0x00 => {
            return Ok(None)
        },
        n => Format::Unrecognized(n),
    };

    let bootable = entry[0] >= 0x80;
    let start = (&entry[8..12]).read_le_u32().unwrap();
    let size = (&entry[12..16]).read_le_u32().unwrap();

    let pinfo = PartitionInfo {
        format: format,
        start: start as usize,
        size: size as usize,
        bootable: bootable,
    };

    Ok(Some(pinfo))
}

pub fn set_pinfo<T: Disk>(disk: &mut T, index: usize, pinfo: &PartitionInfo) -> Result<()> {
    use std::slice::bytes::copy_memory;
    // get old MBR
    let mut mbr = try!(disk.read_sector(0)).clone();

    {
        let mut pt = &mut mbr[446..509]; // the 4 * 16 byte partition table
        assert!(index < 4, "Invalid partion table index: {}", index);
        let mut entry = &mut pt[index * 16 .. (index + 1) * 16];

        // zero out old CHS addresses to prevent future confusion
        // TODO: support CHS addressing
        copy_memory(&[0, 0, 0], &mut entry[1..4]);
        copy_memory(&[0, 0, 0], &mut entry[5..8]);

        entry[0] = if pinfo.bootable { 0x80 } else { 0x00 };
        entry[4] = pinfo.format.serialize();
        // TODO: unwrap or look at implementation. Shouldn't fail for &[u8]?
        (&mut entry[8..12]).write_le_u32(pinfo.start as u32);
        (&mut entry[12..16]).write_le_u32(pinfo.size as u32);
    }

    try!(disk.write_sector(0, &mbr));

    Ok(())
}

impl Disk for Partition {
    fn info(&self) -> DiskInfo {
        DiskInfo {
            size: self.size,
            sector_size: 512,
        }
    }
    fn read_sector(&self, lba: usize) -> Result<&Sector> {
        if lba < self.size {
            unsafe { (*self.device).read_sector(self.start + lba) }
        } else {
            Err(Error::BeyondDiskSize)
        }
    }
    fn write_sector(&mut self, lba: usize, data: &[u8]) -> Result<()> {
        use self::Disk;
        if lba < self.size {
            unsafe { (*self.device).write_sector(self.start + lba, data) }
        } else {
            Err(Error::BeyondDiskSize)
        }
    }
}


