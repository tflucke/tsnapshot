pub mod tar;
pub mod copy;
pub mod hardlink;

use std::path::*;

pub trait BackupOutputStream {
    fn append_file(&mut self, src: &Path) -> Result<(), std::io::Error>;
}

fn append_path(dst: &Path, src: &Path) -> PathBuf { return dst.join(as_relative(src)) }

fn as_relative(path: &Path) -> PathBuf {
    if path.is_relative() {
        return path.to_path_buf()
    }
    else {
        return path.components().flat_map(|comp| match comp {
            // [tflucke] 2021-12-30 TODO: Haven't put any actual effort into making sure this is clean.
            Component::Prefix(prefix) => Some(Component::Normal(prefix.as_os_str())),
            Component::RootDir        => None,
            Component::CurDir         => None,
            Component::ParentDir      => Some(Component::ParentDir),
            Component::Normal(s)      => Some(Component::Normal(s))
        }).collect()
    }
}
