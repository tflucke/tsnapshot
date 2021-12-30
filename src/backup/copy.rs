use std::path::Path;
use std::fs;
use crate::backup::{self,BackupOutputStream};

// ----- Public Data Structures ------------------------------------------------

pub struct CopyOutputStream<'a> {
    pub output_dir:    &'a Path,
}

impl <'a> CopyOutputStream<'a> {
    pub fn new(output_dir: &'a Path) -> CopyOutputStream<'a> {
        return CopyOutputStream {
            output_dir:    output_dir,
        }
    }
}

impl <'a> BackupOutputStream for CopyOutputStream<'a> {
    fn append_file(&mut self, src: &Path) -> Result<(), std::io::Error> {
        let meta = src.symlink_metadata()?;
        let output_file = backup::append_path(self.output_dir, src);
        if meta.file_type().is_symlink() {
            log::debug!("Backing up symlink {:?} to {:?}.", src, self.output_dir);
            symlink::symlink_auto(output_file, fs::read_link(src)?)?;
        }
        else if meta.file_type().is_dir() {
            log::debug!("Backing up directory {:?} to {:?}.", src, self.output_dir);
            fs::create_dir(&output_file)?;
        }
        else if meta.file_type().is_file() {
            log::debug!("Backing up file {:?} to {:?}.", src, self.output_dir);
            fs::copy(src, output_file)?;
        }
        else {
            // [tflucke] 2021-12-24: Unknown type.  Probably added after this was written.
            unimplemented!("Unknown entry type detected {:?}.", src);
        }
        return Ok(());
    }
}
