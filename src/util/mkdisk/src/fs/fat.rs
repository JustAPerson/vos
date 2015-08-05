use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use disk::{Disk, Sector, EMPTY_SECTOR, Result, Error};
use fs::FileSystem;

pub struct Fat32 {
    disk: Box<Disk>, // Underlying medium
    fat_begin: usize, // LBA of first FAT
    cluster_begin: usize, // LBA address of first cluster
    cluster_size: usize, // Size of cluster in sectors
    rdir_cluster: usize,  //Root directory cluster
}

enum FatEntry {
    Free,
    Cont(u32),
    End,
    Bad,
    Reserved(u32),
}

// TODO: handle file attributes
const IS_SUBDIR: u8 = 1 << 4;
#[derive(Debug, Clone, PartialEq)]
enum DirEntry {
    End,
    Free,
    Dir {
        // TODO: rename `name` -> `stem`
        name: String,
        ext: String,
        start: usize,
    },
    File {
        name: String,
        ext: String,
        start: usize,
        size: usize,
    }
}
impl DirEntry {
    fn name(&self) -> Option<&str> {
        let name: Option<&str> = match *self {
            DirEntry::Dir { ref name, ..} => Some(name),
            DirEntry::File { ref name, ..} => Some(name),
            _ => None,
        };
        match name {
            // essentially no name
            Some("") => None,
            Some(s) => Some(s),
            None => None,
        }
    }
    fn ext(&self) -> Option<&str> {
        let ext: Option<&str> = match *self {
            DirEntry::Dir { ref ext, ..} => Some(ext),
            DirEntry::File { ref ext, ..} => Some(ext),
            _ => None,
        };
        match ext {
            Some("") => None, // essentially no extension
            Some(s) => Some(s),
            None => None,
        }
    }
    fn start(&self) -> Option<usize> {
        match *self {
            DirEntry::Dir { start, ..} => Some(start),
            DirEntry::File { start, ..} => Some(start),
            _ => None,
        }
    }
    fn size(&self) -> Option<usize> {
        match *self {
            DirEntry::File { size, ..} => Some(size),
            _ => None,
        }
    }
}

impl Fat32 {
    pub fn new(disk: Box<Disk>) -> Result<Fat32> {
        let fat_begin;
        let cluster_begin;

        {
            let header = try!(disk.read_sector(0));
            fat_begin = (&header[14..16]).read_u16::<LittleEndian>().unwrap() as usize;

            let fats = (&header[16..17]).read_u8().unwrap() as usize;
            let fsize = (&header[36..40]).read_u32::<LittleEndian>().unwrap() as usize;
            cluster_begin = fat_begin + fats*fsize;
        }

        Ok(Fat32 {
            disk: disk,
            fat_begin: fat_begin,
            cluster_begin: cluster_begin,
            cluster_size: 1,
            rdir_cluster: 2,
        })
    }

    // TODO: should Fat32::read_cluster() even return Result???
    fn read_cluster(&self, c: usize) -> Result<&Sector> {
        debug!("read_cluster c=0x{:x} r=0x{:x}", c, self.cluster_begin + (c - 2) * self.cluster_size);
        let r = self.disk.read_sector(self.cluster_begin + (c - 2) * self.cluster_size);
        assert!(r.is_ok(), "Fat32: Invalid cluster `0x{:x}`", c);
        r
    }

    fn write_cluster(&mut self, c: usize, data: &[u8]) -> Result<()> {
        let r = self.disk.write_sector(self.cluster_begin + (c - 2) * self.cluster_size, data);
        assert!(r.is_ok(), "Fat32: Invalid cluster `0x{:x}`", c);
        r
    }

    /// Read FAT entry
    ///
    /// Returns the FAT entry of the specified cluster
    fn read_fate(&self, c: usize) -> Result<FatEntry> {
        let lba = self.fat_begin + (c / 128);
        let offset = c % 128;

        let fat_sector = try!(self.disk.read_sector(lba));
        const MASK: u32 = 0x0fffffff;
        let entry = MASK & (&fat_sector[offset..offset+4]).read_u32::<LittleEndian>().unwrap();

        debug!("read_fate cluster=0x{:x} lba=0x{:x} offset=0x{:x} entry=0x{:x}", c, lba, offset, entry);

        let status = match entry {
            // TODO: reorganize or something so that hot code path is first
            n if n >= 0x0FFFFFF8 => { FatEntry::End },
            n if n == 0x0FFFFFF7 => { FatEntry::Bad },
            n if n >= 0x0FFFFFF0 => { FatEntry::Reserved(n) },
            n if n >= 0x00000002 => { FatEntry::Cont(n) },
            n if n == 0x00000001 => { FatEntry::Reserved(n) },
            n if n == 0x00000000 => { FatEntry::Free},
            _ => unreachable!(),
        };

        Ok(status)
    }

    /// Write FAT entry
    ///
    /// Writes the FAT entry for the specified cluster
    // TODO: resolve inconsistent naming with write_fate versus set_dire
    fn write_fate(&mut self, c: usize, fate: &FatEntry) -> Result<()> {
        let entry = match *fate {
            FatEntry::Cont(n)     => { n },
            FatEntry::Free        => { 0x00000000 },
            FatEntry::End         => { 0x0FFFFFFF },
            FatEntry::Bad         => { 0x0FFFFFF7 },
            FatEntry::Reserved(n) => { n },
        };

        let lba = self.fat_begin + c / 128;
        let offset = c % 128;

        let mut fat_sector = try!(self.disk.read_sector(lba)).clone();
        (&mut fat_sector[offset..offset+4]).write_u32::<LittleEndian>(entry).unwrap();
        try!(self.disk.write_sector(lba, &fat_sector));

        Ok(())
    }

    /// Search the FAT to find the next cluster
    ///
    /// For a file, this will find the next cluster containing the file's
    /// contents. For a directory, this will find the next cluster containing
    /// the directory entries.
    fn next_cluster(&self, cluster: usize) -> Result<Option<usize>> {
        match try!(self.read_fate(cluster)) {
            FatEntry::Cont(n) => Ok(Some(n as usize)),
            _ => Ok(None),
        }
    }

    /// Looks for a child in a directory
    ///
    /// `cluster` should point to the beginning of the directory listing.
    /// All of the parent directories of `child` are ignored. Only the
    /// file name and extension are used.
    fn find_dire_cluster(&self, cluster: usize, child: &Path) -> Result<Option<usize>> {
        // TODO: optimize with `find_dire_index`
        // may involve the optimization mentioned in `find_dire_index`
        // or specializing `find_dire_index` for this case.
        match try!(self.find_dire_index(cluster, child)) {
            Some(i) => {
                // `DirEntry::start()` should always return Some below
                // `find_dire_index()` above already proved the dire is a File/Dir
                // which always have a .start()
                Ok(try!(self.get_dire(cluster, i)).start())
            }
            None => Ok(None), // file/dir not found in directory
        }
    }

    /// Find index of a directory entry
    ///
    /// `cluster` points to the beginning of the directory entry.
    /// Only the file name/extension of `child` will be considered
    /// The result is given relative to `cluster`, which may be greater than
    /// what any individual cluster may hold. `set_dire` / `get_dire` will
    /// adjust the index accordingly.
    fn find_dire_index(&self, mut cluster: usize, child: &Path) -> Result<Option<usize>> {
        // use `dire_index` to keep track of the index if the dir listing
        // spans multiple clusters. The is the index relative to the beginning
        // of the listing. Could potentially return a (usize, usize) that describes
        // both the cluster in the middle of the directory listing and the index
        // within that cluster
        let mut dire_index = 0;
        debug!("find_dire_cluster cluster=0x{:x} path={:?}", cluster, child);
        loop {
            let sector = try!(self.read_cluster(cluster));
            for i in 0..16 {
                let entry = &sector[i * 32 .. (i + 1) * 32];

                debug!("find_dire_index entry[0]=0x{:x}", entry[0]);
                match entry[0] {
                    0x00 => { return Ok(None) } // end of directory
                    0xE5 => { continue } // skip unused entry
                    _ => { },
                }

                // if `name` is shorter than 8 chars, then it's padded with spaces
                // same for `ext`, so remove them.
                // TODO: ensure all paths are sanitized of spaces
                let name = String::from_utf8_lossy(&entry[0..8]);
                let ext = String::from_utf8_lossy(&entry[8..11]);
                let fname = child.file_stem().unwrap_or("".as_ref());
                let fext = child.extension().unwrap_or("".as_ref());

                debug!("find_dire_cluster i={} name={:?} ext={:?} fname={:?} fext={:?}", i, name, ext, fname, fext);

                if *name.trim_right() == *fname && *ext.trim_right() == *fext {
                    debug!("find_dire_cluster match");
                    return Ok(Some(dire_index + i))
                }
            }

            dire_index += 16;
            cluster = match try!(self.next_cluster(cluster)) {
                Some(c) => c,
                None => {
                    // something went wrong
                    // perhaps dir listing is corrupt and we missed an DirEntry::End
                    // perhaps FAT is corrupt.
                    // TODO: read backup FAT
                    return Err(Error::CorruptFAT)
                }
            }
        }
    }

    /// Find a free cluster
    ///
    /// Search FAT for first free FAT entry. Zero the cluster.
    /// Set FAT entry as appropriate. If an `old` cluster address
    /// is given, extend FAT chain. The `old` cluster can poitn anywhere
    /// in the FAT chain (e.g. at the beginning, middle, or end)
    fn alloc_cluster(&mut self, old: Option<usize>) -> Result<usize> {
        // TODO: improve Fat32::alloc_cluster()
        // traversing the FAT linearly will become very slow
        let mut new = self.cluster_begin;
        loop {
            if let FatEntry::Free = try!(self.read_fate(new)) {
                break;
            }
            new += 1;
            // TODO: handle full FS
        }

        // found a cluster, now zero it and set FAT
        try!(self.write_cluster(new, &EMPTY_SECTOR));

        if let Some(mut old) = old {
            // extending existing cluster
            // check if this is the end of the FAT entry chain
            loop {
                match try!(self.read_fate(old)) {
                    FatEntry::End => { break } // at end of chain
                    FatEntry::Cont(c) => { old = c as usize }, // continue chain
                    _ => { // chain broken, unsure what to do here
                        // TODO: read backup FAT
                        return Err(Error::CorruptFAT)
                    }
                }
            }
            // extend FAT chain
            try!(self.write_fate(old, &FatEntry::Cont(new as u32)));
        }
        // ensure FAT chain ends
        try!(self.write_fate(new, &FatEntry::End));

        Ok(new)
    }

    /// Finds a free directory listing entry
    ///
    /// `cluster` should point to the beginning of the directory listing.
    /// The resulting index is given relative to the beginning of the directory
    /// listing (i.e. the index may be larger than an individual cluster will
    /// hold, but `set_dire` etc will account for this)
    fn alloc_dire(&mut self, mut cluster: usize) -> Result<usize> {
        debug!("alloc_dire cluster=0x{:x}", cluster);

        let mut iteration = 0;
        loop {
            for i in 0..16 {
                match try!(self.get_dire(cluster, i)) {
                    DirEntry::Free => {
                        return Ok(iteration * 16 + i)
                    },
                    DirEntry::End => {
                        try!(self.set_dire(cluster, i, &DirEntry::Free));
                        if i < 15 {
                            // cluster has enough room to put DirEntry::End inside
                            try!(self.set_dire(cluster, i + 1, &DirEntry::End));
                        } else {
                            // cluster does not have enough room
                            // allocate new cluster and place DirEntry::End at beginning
                            let new = try!(self.alloc_cluster(Some(cluster)));
                            try!(self.set_dire(new, 0, &DirEntry::End));
                        }
                        return Ok(iteration * 16 + i)
                    }
                    _ => { continue }
                }
            }
            iteration += 1;
            cluster = try!(self.next_cluster(cluster)).expect("Corrupt FAT");
        }
    }

    fn get_dire(&self, mut cluster: usize, mut offset: usize) -> Result<DirEntry> {
        while offset >= 16 {
            // dir entry is not in this cluster
            // so traverse the FAT until the appropriate cluster/offset pair is found
            cluster = try!(self.next_cluster(cluster)).expect("Corrupt FAT");
            offset -= 16;
        }

        let sector = try!(self.read_cluster(cluster));
        let entry = &sector[offset * 32 .. (offset + 1) * 32];
        match entry[0] {
            // TODO: harden against invalid filenames
            // that may contain DirE signals such as these below
            0x00 => { return Ok(DirEntry::End) } // end of directory
            0xE5 => { return Ok(DirEntry::Free) } // skip unused entry
            _ => { },
        }

        let name = String::from_utf8_lossy(&entry[0..8]);
        let ext = String::from_utf8_lossy(&entry[8..11]);

        let attrib = entry[11];

        let cluster_hi = (&entry[20..22]).read_u16::<LittleEndian>().unwrap() as usize;
        let cluster_lo = (&entry[26..28]).read_u16::<LittleEndian>().unwrap() as usize;
        let cluster = cluster_hi << 16 | cluster_lo;

        if attrib & IS_SUBDIR > 0 {
            // subdirectory
            Ok(DirEntry::Dir {
                name: name.into_owned(),
                ext: ext.into_owned(),
                start: cluster,
            })
        } else {
            // file
            let size = (&entry[28..32]).read_u32::<LittleEndian>().unwrap() as usize;
            Ok(DirEntry::File {
                name: name.into_owned(),
                ext: ext.into_owned(),
                start: cluster,
                size: size,
            })
        }
    }

    fn set_dire(&mut self, mut cluster: usize, mut offset: usize, dire: &DirEntry) -> Result<()> {
        while offset >= 16 {
            // dir entry is not in this cluster
            // so traverse the FAT until the appropriate cluster/offset pair is found
            cluster = try!(self.next_cluster(cluster)).expect("Corrupt FAT");
            offset -= 16;
        }

        debug!("set_dire cluster=0x{:x} offset=0x{:x} dire={:?}", cluster, offset, dire);
        let mut sector = try!(self.read_cluster(cluster)).clone();
        {
            let entry = &mut sector[offset * 32 .. (offset + 1) * 32];
            match *dire {
                DirEntry::End  => {
                    entry[0] = 0x00;
                },
                DirEntry::Free => {
                    entry[0] = 0xE5;
                },
                DirEntry::Dir { .. }  | DirEntry::File { .. } => {
                    // Dir & File contain several shared fields
                    use std::iter::repeat;

                    let name = dire.name().unwrap_or("").bytes().chain(repeat(b' ')).take(8);
                    let ext = dire.ext().unwrap_or("").bytes().chain(repeat(b' ')).take(3);
                    let mut i = 0;
                    for byte in name {
                        entry[i] = byte;
                        i += 1;
                    }
                    for byte in ext {
                        entry[i] = byte;
                        i += 1;
                    }

                    if let DirEntry::Dir { .. } = *dire {
                        entry[11] = IS_SUBDIR;
                    }
                    // TODO: handle file attributes

                    let cluster = dire.start().unwrap();
                    let cluster_hi = (cluster & 0xFFFF0000) >> 16;
                    let cluster_lo =  cluster & 0x0000FFFF;
                    debug!("set_dire cluster=0x{:x} hi=0x{:x} lo=0x{:x}", cluster, cluster_hi, cluster_lo);
                    (&mut entry[20..22]).write_u16::<LittleEndian>(cluster_hi as u16).unwrap();
                    (&mut entry[26..28]).write_u16::<LittleEndian>(cluster_lo as u16).unwrap();

                    if let DirEntry::File { size, .. } = *dire {
                        // File also specifies a filesize
                        (&mut entry[28..32]).write_u32::<LittleEndian>(size as u32).unwrap();
                    }
                }
            }
        }
        try!(self.write_cluster(cluster, &sector));

        Ok(())
    }

    fn find_dir(&self, path: &Path) -> Result<usize> {
        let mut cluster = self.rdir_cluster;
        let mut iter = path.iter();
        debug!("find_dir path={:?}", path);
        loop {
            let item = match iter.next() {
                Some(item) => item,
                None => { break },
            };
            debug!("find_dir item={:?}", item);
            match try!(self.find_dire_cluster(cluster, item.as_ref())) {
                Some(c) => { cluster = c },
                None => {
                    let count = iter.count();
                    let mut epath = path;
                    for _ in 0..count {
                        // unwrap() should be fine here, see Path::parent() docs
                        epath = epath.parent().unwrap();
                    }
                    return Err(Error::Nonexistent(epath.to_owned()))
                }
            }
        }
        Ok(cluster)
    }

    fn find_parent_dir(&self, path: &Path) -> Result<usize> {
        match path.parent() {
            Some(parent) => self.find_dir(parent),
            None => Ok(self.rdir_cluster),
        }
    }
}


impl FileSystem for Fat32 {
    fn make_dir(&mut self, path: &Path) -> Result<()> {
        let mut cluster = try!(self.find_parent_dir(path));

        // alloc a cluster for the directory listing
        // and prepare the list
        let new_c = try!(self.alloc_cluster(None));
        try!(self.set_dire(new_c, 0, &DirEntry::End));

        // now find an entry in the parent directory's listing
        let direi = try!(self.alloc_dire(cluster));

        let name = try!(normalize_stem(path)).to_owned();
        let ext = try!(normalize_ext(path)).to_owned();
        let dire = DirEntry::Dir {
            name: name,
            ext: ext,
            start: new_c,
        };

        debug!("make_dir cluster=0x{:x} direi=0x{:x}", cluster, direi);
        try!(self.set_dire(cluster, direi, &dire));

        Ok(())
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        Ok(())
    }
    fn write_file(&mut self, path: &Path, buf: &[u8]) -> Result<()> {
        use std::iter::repeat;

        // find the directory and where the file exists within it
        let dcluster = try!(self.find_parent_dir(path));
        let (direi, mut fcluster) = match try!(self.find_dire_index(dcluster, path)) {
            // File exists, overwriting
            Some(i) => {
                // unwrap() should be safe, see Fat32::find_dire_cluster()
                (i, try!(self.get_dire(dcluster, i)).start().unwrap())
            },
            // File does not exist, alloc dire / cluster
            None => {
                (try!(self.alloc_dire(dcluster)), try!(self.alloc_cluster(None)))
            }
        };

        // update dir entry
        // TODO sanitize paths
        let fname = try!(normalize_stem(path));
        let fext = try!(normalize_ext(path));
        let dire = DirEntry::File {
            name: fname.chars().chain(repeat(' ')).take(8).collect(),
            ext: fext.chars().chain(repeat(' ')).take(3).collect(),
            start: fcluster,
            size: buf.len(),
        };
        try!(self.set_dire(dcluster, direi, &dire));

        for chunk in buf.chunks(self.cluster_size * 512) { // TODO generc over sector size
            debug!("write_file fcluster=0x{:x}", fcluster);
            try!(self.write_cluster(fcluster, chunk));
            // find where to write next cluster
            fcluster = match try!(self.next_cluster(fcluster)) {
                // file had already allocated enough space in FAT chain
                Some(c) => {
                    debug!("write_file reusing fcluster=0x{:x}", c);
                    c
                },
                // need to extend file's FAT chain
                None => try!(self.alloc_cluster(Some(fcluster))),
            }
        }

        // TODO: free any excess space in FAT chain
        // the logic is a little difficult at 2AM and boring, so do later

        Ok(())
    }
    fn read_file(&mut self, path: &Path, buf: &mut [u8]) -> Result<()> {
        Ok(())
    }
}

fn normalize_stem(path: &Path) -> Result<&str> {
    match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => Ok(s),
        None => {
            // This covers the case where file_stem() is None
            // and when the stem cannot be converted to a utf8 &str
            Err(Error::InvalidPath)
        }
    }
}
fn normalize_ext(path: &Path) -> Result<&str> {
    match path.extension() {
        Some(s) => {
            match s.to_str() {
                Some(s) => Ok(s),
                None => {
                    Err(Error::InvalidPath)
                }
            }
        }
        None => Ok(""),
    }
}

/// Format drive as a FAT32 filesystem
///
/// This will overwrite the first sector, the reserved sectors,
/// and the space used by the FATs. Everything after will be left intact
/// until the FS is mounted and written to.
pub fn format<T: Disk>(disk: &mut T) -> Result<()> {
    // TODO support clusters > 1 sector
    const FATS: usize = 2;
    const RESERVED: usize = 32;
    const SSIZE: usize = 512; // sector size (bytes)
    const CSIZE: usize = 1; // cluster size (sectors)

    let dsize = disk.info().size;
    debug!("format dsize={}", dsize);
    let (fsize, csize) = calc_sizes(dsize, FATS, RESERVED);

    let mut header = Sector([0; 512]);
    let _ = (&mut header[11..13]).write_u16::<LittleEndian>(SSIZE as u16); // bytes per sector
    let _ = (&mut header[13..14]).write_u8(CSIZE as u8); // sector per cluster
    let _ = (&mut header[14..16]).write_u16::<LittleEndian>(RESERVED as u16); // reserved sectors
    let _ = (&mut header[16..17]).write_u8(FATS as u8); // number of FATs
    let _ = (&mut header[19..21]).write_u16::<LittleEndian>(0); // FAT16: total sectors
    let _ = (&mut header[32..36]).write_u32::<LittleEndian>(dsize as u32); // FAT32: total sectors
    let _ = (&mut header[36..40]).write_u32::<LittleEndian>(fsize as u32); // Size of FAT in sectors
    try!(disk.write_sector(0, &header));

    // zero out reserved sectors
    for i in 0..RESERVED {
        try!(disk.write_sector(1 + i, &EMPTY_SECTOR));
    }

    // zero out FATs
    for i in 0..FATS {
        let fat_start = 1 + RESERVED + i * fsize;
        let mut sector = EMPTY_SECTOR.clone();

        // first two entries of FAT are reserved
        // they're reserved because of the whole fat entry format
        // 0x00000000 means free
        // 0x00000001 is reserved
        // so these cluster addresses cannot be used
        (&mut sector[0..4]).write_u32::<LittleEndian>(0x00000001).unwrap();
        (&mut sector[4..8]).write_u32::<LittleEndian>(0x00000001).unwrap();
        (&mut sector[8..12]).write_u32::<LittleEndian>(0x0FFFFFFF).unwrap(); // signal end of root dir
        try!(disk.write_sector(fat_start, &sector));

        for j in 1..fsize {
            try!(disk.write_sector(fat_start + j, &EMPTY_SECTOR));
        }
    }

    Ok(())
}

/// Calculate sizes for various FS structures
///
/// The size of the file allocation tables depends on the size of the disk,
/// so this simple algorithm will approximate appropriate sizes for the FATs
/// and actual cluster groups.
///
/// Results:
/// - size of each FAT in sectors
/// - number of clusters
fn calc_sizes(dsize: usize, fats: usize, reserved: usize) -> (usize, usize) {
    let mut available = dsize - reserved - 1; // -1 for the FS header

    let mut fsize = 0;
    let mut csize = 0;

    while available > 128 + fats {
        fsize += 1;
        csize += 128;
        available -= 128; // number of entries in a FAT sector
        available -= fats; // reserve a sector for each FAT to hold the entries
    }
    if available > fats {
        fsize += 1;
        available -= fats;
        csize += available;
    }

    debug!("calc_size {:?}", (fsize, csize));
    (fsize, csize)
}
