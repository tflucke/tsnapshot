use std::path::Path;
use std::io::{Write,Error,ErrorKind};
use crate::backup::{self, BackupOutputStream};

// ----- Public Data Structures ------------------------------------------------

pub struct TarOutputStream<'a> {
    output: &'a mut dyn Write,
}

impl <'a> TarOutputStream<'a> {
    pub fn new(output: &'a mut dyn Write) -> TarOutputStream<'a> {
        return TarOutputStream {
            output: output
        }
    }
}

impl <'a> BackupOutputStream for TarOutputStream<'a> {    
    fn append_file(&mut self, path: &Path) -> Result<(), Error> {
        log::debug!("Creating tar header for file {:?}...", path);
        let header = Header::new(path)?;
        log::debug!("Outputting header for {:?} to destination...", path);
        let pointer = &header as *const Header as *const u8;
        assert_eq_size!(Header, [u8; 512]);
        self.output.write_all(unsafe {std::slice::from_raw_parts(pointer, 512)})?;
        if path.is_file() {
            log::debug!("{:?} is a file.  Writing contents to destination...", path);
            log::debug!("Opening for reading...");
            let mut file = std::fs::OpenOptions::new().read(true).write(false).open(path)?;
            log::debug!("Copying contents to output...");
            std::io::copy(&mut file, self.output)?;
            // Pad out to 512 bytes with 0s.
            let file_size = path.symlink_metadata()?.len();
            let padding_size = 512 - (file_size as usize % 512);
            log::debug!("Writing out {:?} bytes of 0 padding", padding_size);
            self.output.write_all(&[0; 512][0..padding_size])?;
        }
        return Ok(())
    }
}

impl <'a> Drop for TarOutputStream<'a> {
    fn drop(&mut self) {
        // Write out two empty 512 byte blocks.
        match self.output.write_all(&[0; 1024]) {
            Ok(..)   => {},
            Err(err) => log::error!("Failed to write closing bytes to file: {:?}", err)
        };
    }
}

// ----- Tar Implementation ----------------------------------------------------

// [tflucke] 2021-12-29: TODO: Hard links are hard.  Not sure how to handle this yet,
enum FileType {
    NormalFile,
    //Hardlink,
    Symlink,
    CharSpecial,
    BlockSpecial,
    Directory,
    FIFO
}

impl FileType {
    fn from_metadata(metadata: &std::fs::Metadata) -> FileType {
        {
            use std::os::unix::fs::FileTypeExt;
            if metadata.file_type().is_block_device() {
                return FileType::BlockSpecial;
            }
            else if metadata.file_type().is_char_device() {
                return FileType::CharSpecial;
            }
            else if metadata.file_type().is_fifo() {
                return FileType::FIFO;
            }
        }

        if metadata.file_type().is_file() {
            return FileType::NormalFile;
        }
        else if metadata.file_type().is_dir() {
            return FileType::Directory;
        }
        else if metadata.file_type().is_symlink() {
            return FileType::Symlink;
        }
        else {
            unimplemented!("Unknown file type detected.");
        }
    }
    
    fn as_char(&self) -> u8 {
        use FileType::*;
        
        match self {
            NormalFile   => b'0',
            //Hardlink     => b'1',
            Symlink      => b'2',
            CharSpecial  => b'3',
            BlockSpecial => b'4',
            Directory    => b'5',
            FIFO         => b'6'
        }
    }
}

#[repr(C)]
struct Header {
    name:      [u8; 100],
    mode:      [u8; 8],
    uid:       [u8; 8],
    gid:       [u8; 8],
    size:      [u8; 12],
    mtime:     [u8; 12],
    checksum:  [u8; 8],
    typ:       [u8; 1],
    link:      [u8; 100],

    // UStar format
    magic:     [u8; 6],
    version:   [u8; 2],
    owner:     [u8; 32],
    group:     [u8; 32],
    dev_major: [u8; 8],
    dev_minor: [u8; 8],
    prefix:    [u8; 155],
    pad:       [u8; 12],
}

impl Header {
    fn new(path: &std::path::Path) -> Result<Header, Error> {
        let path = backup::as_relative(path);
        let path_string = path.to_str()
            .ok_or(Error::new(ErrorKind::InvalidInput, "Filename not representable as str.")
            )?.to_string();
        let (prefix, name) = if path_string.len() <= 100 {
            ("", path_string)
        }
        else {
            let (a, b) = path_string.split_at(path_string.len() - 100);
            (a, b.to_string())
        };
        let meta = path.symlink_metadata()?;
        let (output_name, size) = if meta.is_dir() {
            (if name.ends_with("/") { name } else { name + "/" }, 0)
        }
        else {
            (name, meta.len())
        };
        
        let mut res = Header {
            name:      [0; 100],
            mode:      [0; 8],
            uid:       [0; 8],
            gid:       [0; 8],
            size:      [0; 12],
            mtime:     [0; 12],
            checksum:  [b' '; 8],
            typ:       [0],
            link:      [0; 100],
            magic:     [b'u', b's', b't', b'a', b'r', b' '],
            version:   [b' ', b'\0'],
            owner:     [0; 32],
            group:     [0; 32],
            dev_major: [0; 8],
            dev_minor: [0; 8],
            prefix:    [0; 155],
            pad:       [0; 12]
        };
        byte_copy(output_name.as_bytes(), &mut res.name);
        {
            use std::os::unix::fs::MetadataExt;
            byte_copy(format!("{:07o}", meta.mode() & 0o7777u32).as_bytes(), &mut res.mode);
            byte_copy(format!("{:07o}", meta.uid()).as_bytes(), &mut res.uid);
            byte_copy(format!("{:07o}", meta.gid()).as_bytes(), &mut res.gid);
        }
        byte_copy(format!("{:011o}", size).as_bytes(), &mut res.size);
        byte_copy(format!("{:011o}", meta
                          .modified()?
                          .duration_since(std::time::UNIX_EPOCH)
                          .map_err(|err| Error::new(ErrorKind::Other, err))?
                          .as_secs()
        ).as_bytes(), &mut res.mtime);        
        res.typ[0] = FileType::from_metadata(&meta).as_char();
        match std::fs::read_link(path).map(|x| byte_copy(x.to_str().unwrap().as_bytes(), &mut res.link)) {
            Ok(..) => {}
            Err(..) => {/* Ignore Errors */}
        };

        {
            use std::os::unix::fs::MetadataExt;
            let uid = users::get_user_by_uid(meta.uid())
                .ok_or(Error::new(ErrorKind::Other, "Cannot get UID"))?;
            byte_copy(uid.name().to_str()
                      .ok_or(Error::new(ErrorKind::InvalidInput, "Username not representable as str."))?
                      .as_bytes(), &mut res.owner);
            let gid = users::get_group_by_gid(meta.gid())
                .ok_or(Error::new(ErrorKind::Other, "Cannot get GID"))?;
            byte_copy(gid.name().to_str()
                      .ok_or(Error::new(ErrorKind::InvalidInput, "Groupname not representable as str."))?
                      .as_bytes(), &mut res.group);
            // [tflucke] 2021-12-17: TODO: Fix device number
            // [tflucke] 2021-12-29: Looks like GNU tar leaves this empty?  Maybe it's just for special devices?
            //byte_copy(format!("{:o}", meta.dev()).as_bytes(), &mut res.dev_major);
        }
        
        byte_copy(prefix.as_bytes(), &mut res.pad);

        // Calculate checksum
        let checksum: u32 =
            res.name.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.mode.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.uid.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.gid.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.size.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.mtime.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.checksum.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.typ.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.link.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.magic.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.version.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.owner.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.group.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.dev_major.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.dev_minor.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.prefix.iter().map(|u| u32::from(*u)).sum::<u32>() +
            res.pad.iter().map(|u| u32::from(*u)).sum::<u32>();
        byte_copy(format!("{:06o}\0", checksum).as_bytes(), &mut res.checksum);
        
        return Ok(res)
    }
}

fn byte_copy(from: &[u8], mut to: &mut [u8]) -> usize {
    to.write(from).unwrap()
}
