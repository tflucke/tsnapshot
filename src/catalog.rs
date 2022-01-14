use crate::config::KeepLimit;

// ----- Public Data Structures ------------------------------------------------

#[derive(Debug)]
pub struct BackupCatalog {
    entries: Vec<Entry>
}

impl BackupCatalog {
    pub fn new(file: std::fs::File) -> Result<BackupCatalog,Error> {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(file);
        Ok(BackupCatalog {
            entries: reader.lines().map(Entry::new).collect::<Result<Vec<Entry>, Error>>()?
        })
    }
    
    pub fn empty() -> BackupCatalog {
        BackupCatalog {
            entries: vec![]
        }
    }
    
    pub fn save_to(&self, file: std::fs::File) -> Result<(), std::io::Error>{
        use std::io::Write;
        let mut writer = std::io::BufWriter::new(file);
        for entry in &self.entries {
            writeln!(writer, "{} {}", entry.path.to_str().unwrap(), entry.timestamp)?;
        }
        Ok(())
    }
    
    pub fn push(&mut self, path: &std::path::Path) {
        self.entries.push(Entry {
            path: path.to_path_buf(),
            timestamp: chrono::offset::Utc::now().timestamp()
        })
    }
    
    pub fn most_recent(&self) -> Option<&std::path::Path> {
        if self.entries.len() > 0 {
            Some(&self.entries[0].path)
        }
        else {
            None
        }
    }

    pub fn clean(&mut self, keep_limits: Vec<KeepLimit>) -> Result<(), std::io::Error> {
        if 0 == self.entries.len() {
            return Ok(())
        }
        let mut iter = self.entries.iter();
        let mut last_backup = match iter.next() {
            Some(backup) => backup.timestamp,
            None         => return Ok(())
        };
        for limit in keep_limits {
            for _ in 0 .. limit.count {
                match BackupCatalog::clean_to_timestamp(&mut iter, last_backup - limit.timespan as i64) {
                    Some(next_backup) => last_backup = next_backup.timestamp,
                    // [tflucke] 2021-12-30: Nothing left to clean up
                    None              => return Ok(())
                }
            }
        }
        BackupCatalog::clean_to_timestamp(&mut iter, 0);
        Ok(())
    }

    fn clean_to_timestamp<'a>(iter: &'a mut std::slice::Iter<Entry>, timestamp: i64) -> Option<&'a Entry> {
        for entry in iter {
            if entry.timestamp > timestamp {
                // [tflucke] 2021-12-30: next_backup is too recent.  Remove it.
            }
            else {
                return Some(entry)
            }
        }
        return None
    }
}

type Error = CatalogError;

#[derive(Debug)]
pub enum CatalogError {
    ParseError,
    IoError(std::io::Error)
}

// ----- Entry Parsing Functions -----------------------------------------------

#[derive(Debug)]
struct Entry {
    path: std::path::PathBuf,
    timestamp: i64
}

impl Entry {
    fn new(res: Result<std::string::String, std::io::Error>) -> Result<Entry, Error> {
        use regex::Regex;
        lazy_static! {
            static ref R: Regex = Regex::new(r"^(.+)\s+(\d+)$").unwrap();
        }
        let string = res.map_err(|err| CatalogError::IoError(err))?;
        log::debug!("Entry line: {}", string);
        let caps = R.captures(string.as_str())
            .ok_or(CatalogError::ParseError)?;
        Ok(Entry {
            path: std::path::Path::new(caps.get(1).ok_or(CatalogError::ParseError)?.as_str()).to_path_buf(),
            timestamp: caps.get(2).ok_or(CatalogError::ParseError)?.as_str().parse()
                .map_err(|_| CatalogError::ParseError)?,
        })
    }
}

impl ToString for Entry {
    fn to_string(&self) -> std::string::String {
        return format!("{} {}", self.path.to_str().unwrap(), self.timestamp);
    }
}
