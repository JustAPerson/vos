use std::ops::Deref;

use disk::{Sector, EMPTY_SECTOR, Disk, DiskInfo, Error, Result};
use byteorder::{WriteBytesExt};

pub struct RamDisk {
    sectors: Vec<Sector>,
}

impl RamDisk {
    /// Creates a new Disk stored in memory
    ///
    /// The disk contains `size` sectors.
    /// To create a 1MiB disk, use `size = 2048`
    pub fn new(size: usize) -> RamDisk {
        assert!(size > 1, "Invalid disk size `{}`: must be greater than 1", size);

        let mut sectors = Vec::with_capacity(size);
        for _ in 0..size {
            sectors.push(EMPTY_SECTOR.clone());
        }

        (&mut sectors[0][510..512]).write_le_u16(0xAA55).unwrap();

        RamDisk { sectors: sectors }
    }
}

impl Disk for RamDisk {
    fn info(&self) -> DiskInfo {
        DiskInfo {
            size: self.sectors.len(),
            sector_size: 512,
        }
    }
    fn read_sector(&self, lba: usize) -> Result<&Sector> {
        if lba <= self.sectors.len() {
            Ok(&self.sectors[lba])
        } else {
            Err(Error::BeyondDiskSize)
        }
    }
    fn write_sector(&mut self, lba: usize, data: &[u8]) -> Result<()> {
        while lba > self.sectors.len() {
            self.sectors.push(Sector([0; 512]));
        }

        if data.len() > 512 { // TODO: Drive API generic over sector size
            Err(Error::WriteError)
        } else {
            // TODO: refactor
            let mut buf = [0; 512];
            let mut i = 0;
            for x in data.iter().take(512) {
                buf[i] = *x;
                i += 1;
            }

            self.sectors[lba] = Sector(buf);
            Ok(())
        }
        // match data.read(&mut self.sectors[lba].0) {
        //     Ok(_) => Ok(()),
        //     Err(_) => Err(Error::WriteError), // TODO: Drive api errors
        // }
    }
}

impl Deref for RamDisk {
    type Target = [Sector];
    fn deref(&self) -> &[Sector] {
        &self.sectors
    }
}
