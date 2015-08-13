use disk::{Disk, EMPTY_SECTOR, DiskInfo, Result, Sector};

pub struct BiosDisk;

impl BiosDisk {
    pub fn new() -> BiosDisk {
        BiosDisk
    }
}

#[repr(packed)]
struct LbaQ {
    size: u8,
    zero: u8,
    sectors: u16,
    buffer: *const u8,
    start: usize,
    upper: usize,
}

static mut LBAQ: LbaQ = LbaQ {
    size: 16,
    zero: 0,
    sectors: 1,
    buffer: 0 as *mut u8,
    start: 0,
    upper: 0,
};

impl Disk for BiosDisk {
    fn info(&self) -> DiskInfo {
        DiskInfo {
            size: 0,
            sector_size: 0,
        }
    }

    // TODO rewrite Disk to return Borrow<Sector>
    fn read_sector(&self, lba: usize) -> Result<&Sector> {
        let sector = Box::new(EMPTY_SECTOR);
        unsafe {
            LBAQ.sectors = 1;
            LBAQ.start = lba;
            LBAQ.buffer = ::std::mem::transmute(&*sector);

            // TODO fix drive number
            // TODO handle disk write error
            asm!("mov ah, 0x42
                  mov dl, 0x80
                  int 0x13"
                  : // no outputs
                  : "{si}"(&LBAQ)
                  : "ax", "dx"
                  : "intel");

            let xmute: *const u8 = ::std::mem::transmute(&*sector);
            let sector: &'static Sector = ::std::mem::transmute(xmute);
            Ok(sector)
        }
    }

    fn write_sector(&mut self, lba: usize, buf: &[u8]) -> Result<()> {
        let sector = EMPTY_SECTOR;
        unsafe {
            LBAQ.sectors = 1;
            LBAQ.start = lba;
            LBAQ.buffer = buf.as_ptr();

            // TODO fix drive number
            // TODO handle disk write error
            asm!("mov ah, 0x43
                  mov dl, 0x80
                  int 0x13"
                  : // no outputs
                  : "{si}"(&LBAQ)
                  : "ax", "dx"
                  : "intel");

            Ok(())
        }
    }
}
