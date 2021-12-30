use std::path::{Path,PathBuf};
use crate::backup::{self, BackupOutputStream};
use crate::backup::copy::CopyOutputStream;
use std::fs::Metadata;

// ----- Public Data Structures ------------------------------------------------

pub struct HardLinkOutputStream<'a> {
    fallback:         CopyOutputStream<'a>,
    ref_dir:          &'a Path,
    max_link_count:   u64,
    detection_method: &'a ChangeDetectionMethod
}

impl <'a> HardLinkOutputStream<'a> {
    pub fn new(output_dir: &'a Path,
               ref_dir: &'a Path,
               max_link_count: u64,
               detection_method: &'a ChangeDetectionMethod
    ) -> HardLinkOutputStream<'a> {
        return HardLinkOutputStream {
            fallback:         CopyOutputStream::new(output_dir),
            ref_dir:          ref_dir,
            max_link_count:   max_link_count,
            detection_method: detection_method
        }
    }
    
    fn equivalent_path(&self, src: &Path, meta: &Metadata) -> Result<Option<PathBuf>, std::io::Error> {
        let other_src = backup::append_path(self.ref_dir, src);
        if !other_src.exists() {
            return Ok(None);
        }
        let other_meta = other_src.metadata()?;
        if self.detection_method.has_changed(src, meta, &other_src, &other_meta) {
            return Ok(None);
        }
        if self.max_link_count < u64::MAX {
            use std::os::unix::fs::MetadataExt;
            if other_meta.nlink() >= self.max_link_count {
                return Ok(None);
            }
        }
        Ok(Some(other_src))
    }
}

impl <'a> BackupOutputStream for HardLinkOutputStream<'a> {
    fn append_file(&mut self, src: &Path) -> Result<(), std::io::Error> {
        let meta = src.symlink_metadata()?;
        let output_file = backup::append_path(self.fallback.output_dir, src);
        if meta.file_type().is_dir() {
            return self.fallback.append_file(src)
        }
        else {
            match self.equivalent_path(src, &meta)? {
                Some(path) => std::fs::hard_link(path, output_file),
                None       => return self.fallback.append_file(src)
            }
        }
    }
}

#[derive(Debug)]
pub enum ChangeDetectionMethod {
    Timestamp,
    FullCompare
}

// ----- Implementation --------------------------------------------------------

impl ChangeDetectionMethod {
    fn has_changed(&self, _src: &Path, meta: &Metadata, _other_src: &Path, other_meta: &Metadata) -> bool {
        match self {
            ChangeDetectionMethod::Timestamp   => return match (meta.modified(), other_meta.modified()) {
                (Ok(mtime), Ok(other_mtime)) => mtime < other_mtime,
                // [tflucke] 2021-12-30: Failed to get one or more modification times.  Assume it changed.
                // Maybe fall back on fullcompare?
                (_,          _)              => true
            },
            ChangeDetectionMethod::FullCompare => todo!()
        }
    }
}
