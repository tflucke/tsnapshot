use std::vec::Vec;
use std::path::{Path,PathBuf};
use regex::Regex;
use json::JsonValue;
use std::fs;
use crate::backup::BackupOutputStream;
use crate::backup::copy::CopyOutputStream;
use crate::backup::tar::TarOutputStream;
use crate::backup::hardlink::{HardLinkOutputStream,ChangeDetectionMethod};
use crate::compression::*;

// ----- Public Data Structures ------------------------------------------------

#[derive(Debug)]
pub struct Configuration {
    pub root_dir_config: Box<dyn DirectoryConfig>,
    pub destination_dir: PathBuf,
    pub verbosity:       log::LevelFilter,
    pub name_format:     std::string::String,
    pub keep_limit:      Vec<KeepLimit>,
    // TODO: pre/post exec commands
    // [tflucke] 2021-12-30: Need to parse everything first,
    // then construct a std::process::Command object in `backup`,
    // then execute it.
    // TODO: Remote sync
}

impl Configuration {
    pub fn new(filename: &str) -> Result<Box<Configuration>, Error> {
        match json::parse(filename) {
            Ok(json)      => match json {
                json::JsonValue::Object(obj) => Ok(Box::new(Configuration {
                    root_dir_config: <dyn DirectoryConfig>::new(
                        obj.get("root_dir_config").ok_or(ParseError::RequiredPropMissing("root_dir_config"))?
                    )?.0,
                    destination_dir: PathBuf::from(str_from_json_prop(&obj, "destination_dir")?),
                    verbosity:       log_level_from_str(str_from_opt_json_prop(&obj, "verbosity", "warn")?)?,
                    name_format:     str_from_opt_json_prop(&obj, "name_format", "%Y-%m-%d_%H-%M-%S")?.to_string(),
                    keep_limit:      KeepLimit::new_vec(obj.get("keep limit"))?
                })),
                _                            => Err(ParseError::NotAnObject(""))
            },
            Err(json_err) => Err(ParseError::JsonError(json_err))
        }
    }

    pub fn backup(&self, ref_path: Option<&Path>) -> Result<PathBuf, std::io::Error> {
        let format = self.name_format.as_str();
        log::info!("output directory format: {:?}", format);
        let now = chrono::Local::now();
        let dst = self.destination_dir.join(Path::new(&now.format(format).to_string()));
        fs::create_dir(&dst)?;
        let mut out = CopyOutputStream::new(&dst);
        self.root_dir_config.backup(self.root_dir_config.get_subpath(), &dst, &mut out, ref_path)?;
        return Ok(dst)
    }
}

pub trait DirectoryConfig: std::fmt::Debug {
    fn backup(&self, src: &Path, dst: &Path, out: &mut dyn BackupOutputStream, last: Option<&Path>)
              -> Result<(), std::io::Error>;
    fn get_subpath(&self) -> &Path;
    fn get_subconfig(&self, path: &Path) -> Option<&dyn DirectoryConfig>;
}

#[derive(Debug,Eq,PartialEq)]
pub struct KeepLimit {
    pub count: u64,
    pub timespan: u64
}

pub type Error = ParseError;

#[derive(Debug)]
pub enum ParseError {
    JsonError(json::Error),
    NotAnObject(&'static str),
    NotAString(&'static str),
    NotAnArray(&'static str),
    NotAnUnsignedInt(&'static str),
    UnknownOption(std::string::String),
    RequiredPropMissing(&'static str),
    BadRegex(std::string::String, regex::Error),
    CannotCompressNonbasic,
}

// ----- Json Parsing Functions ------------------------------------------------

impl KeepLimit {
    fn new_vec(json: Option<&json::JsonValue>) -> Result<Vec<KeepLimit>, Error> {
        match json {
            None                         => Ok(vec![]),
            Some(JsonValue::Array(vec))  => {
                let mut out = vec.iter().map(KeepLimit::new).collect::<Result<Vec<KeepLimit>, Error>>()?;
                out.sort();
                Ok(out)
            },
            Some(..)                     => Err(ParseError::NotAnArray("keep limit"))
        }
    }

    fn new(json: &JsonValue) -> Result<KeepLimit, Error> {
        match json {
            JsonValue::Object(obj) => Ok(KeepLimit {
                count:    uint_from_json_prop(obj, "count")?,
                timespan: KeepLimit::parse_timespan(obj.get("timespan"))?
            }),
            _                     => Err(ParseError::NotAnObject("keep limit"))
        }
    }

    fn parse_timespan(json: Option<&json::JsonValue>) -> Result<u64, Error> {
        match json {
            Some(JsonValue::Object(obj)) => {
                let seconds = uint_from_opt_json_prop(obj, "seconds", 0)?;
                let minutes = uint_from_opt_json_prop(obj, "minutes", 0)?;
                let hours   = uint_from_opt_json_prop(obj, "hours", 0)?;
                let days    = uint_from_opt_json_prop(obj, "days", 0)?;
                let months  = uint_from_opt_json_prop(obj, "months", 0)?;
                let years   = uint_from_opt_json_prop(obj, "years", 0)?;
                return Ok(
                    seconds +
                        minutes * 60 +
                        hours * 60 * 60 +
                        days * 24 * 60 * 60 +
                        months * 30 * 24 * 60 * 60 +
                        years * 365 * 24 * 60 * 60
                )},
            Some(_)                     => Err(ParseError::NotAnObject("timespan")),
            None                        => Err(ParseError::RequiredPropMissing("timespan"))
        }
    }
}

impl std::cmp::Ord for KeepLimit {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        
        if self.timespan < other.timespan {
            Ordering::Less
        }
        else if self.timespan > other.timespan {
            Ordering::Greater
        }
        else if self.count > other.count {
            Ordering::Less
        }
        else if self.count < other.count {
            Ordering::Greater
        }
        else {
            Ordering::Equal
        }
    }
}

impl std::cmp::PartialOrd for KeepLimit {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
}

impl dyn DirectoryConfig {
    fn new<'a>(json: &'a JsonValue) -> Result<(Box<dyn DirectoryConfig>, bool), Error> {
        match json {
            json::JsonValue::Object(obj) => {
                if let Some(space_mode) = obj.get("space_mode") {
                    match space_mode.as_str() {
                        Some("none")     => {
                            let (config, is_only_basics) = BasicDirectory::new(obj)?;
                            Ok((Box::new(config), is_only_basics))
                        }
                        Some("compress") => { CompressedDirectory::new(obj).map(|config| (config, false)) }
                        Some("linked")   => { HardLinkedDirectory::new(obj).map(|config| (config, false)) }
                        Some(typ)        => { Err(ParseError::UnknownOption(typ.to_string())) }
                        None             => { Err(ParseError::NotAString("space_mode")) }
                    }
                }
                else {
                    let (config, is_only_basics) = BasicDirectory::new(obj)?;
                    Ok((Box::new(config), is_only_basics))
                }
            },
            _ => Err(ParseError::NotAnObject(""))
        }
    }

    fn new_vec(json: Option<&json::JsonValue>) -> Result<(Vec<Box<dyn DirectoryConfig>>, bool), Error> {
        match json {
            None => Ok((vec![], true)),
            Some(JsonValue::Array(vec)) => {
                let subconfig_results: Result<Vec<(Box<dyn DirectoryConfig>, bool)>, ParseError> =
                    vec.iter().map(<dyn DirectoryConfig>::new).collect();
                let (configs, is_only_basics): (Vec<Box<dyn DirectoryConfig>>, Vec<bool>) =
                    subconfig_results?.into_iter().unzip();
                return Ok((configs, is_only_basics.iter().all(|is_only_basic| *is_only_basic)));
            },
            Some(..) => Err(ParseError::NotAnArray("subconfigs"))
        }
    }
}

#[derive(Debug)]
pub struct BasicDirectory {
    subpath:    PathBuf,
    subconfigs: Vec<Box<dyn DirectoryConfig>>,
    filters:    Vec<Filter>,
}

impl BasicDirectory {
    fn new(obj: &json::object::Object) -> Result<(BasicDirectory, bool), Error> {
        let (subconfigs, is_only_basic) = <dyn DirectoryConfig>::new_vec(obj.get("subconfigs"))?;
        Ok((BasicDirectory {
            subpath:    PathBuf::from(str_from_json_prop(&obj, "subpath")?),
            subconfigs: subconfigs,
            filters:    Filter::new_vec(obj.get("filters"))?
        }, is_only_basic))
    }
}

impl DirectoryConfig for BasicDirectory {
    fn backup(&self, src: &Path, dst: &Path, out: &mut dyn BackupOutputStream, last: Option<&Path>)
              -> Result<(), std::io::Error> {
        log::debug!("Reading metadata for {:?}...", src);
        let meta = src.symlink_metadata()?;
        log::debug!("Checking if {:?} should be filtered...", src);
        for filter in &self.filters {
            if filter.matches(src, &meta) {
                log::debug!("Skipping {:?} because it matches filter {:?}", src, filter);
                return Ok(())
            }
        }
        if let Some(new_config) = self.get_subconfig(src) {
            log::debug!("Backing up {:?} using new config {:?}...",
                        src, new_config.get_subpath());
            new_config.backup(src, dst, out, last)?;
        }
        else {
            log::debug!("Sending {:?} to backup stream...", src);
            out.append_file(src)?;
            if meta.file_type().is_dir() {
                log::debug!("{:?} is directory.  Backing up subdirectories...", src);
                for entry_res in fs::read_dir(src)? {
                    self.backup(&entry_res?.path(), dst, out, last)?;
                }
            }
        }
        Ok(())
    }
    
    fn get_subpath(&self) -> &Path { return &self.subpath; }

    fn get_subconfig(&self, path: &Path) -> Option<&dyn DirectoryConfig> {
        return match path.strip_prefix(&self.subpath) {
            Ok(path_sans_prefix) => self.subconfigs.iter()
                .find(|config| config.get_subpath() == path_sans_prefix)
                .map(|item| item.as_ref()),
            Err(_)               => None
        };
    }
}

#[derive(Debug)]
struct CompressedDirectory {
    config:     BasicDirectory,
    algorithm:  CompressionAlgorithm
}

impl CompressedDirectory {
    fn new(obj: &json::object::Object) -> Result<Box<dyn DirectoryConfig>, Error> {
        let (config, is_only_basic) = BasicDirectory::new(obj)?;
        if !is_only_basic {
            return Err(ParseError::CannotCompressNonbasic);
        }
        else {
            return Ok(Box::new(CompressedDirectory {
                config:     config,
                algorithm:  str_from_opt_json_prop(&obj, "algorithm", "bzip2")?.parse::<CompressionAlgorithm>()?
            }));
        }
    }
}

impl DirectoryConfig for CompressedDirectory {
    fn backup(&self, src: &Path, dst: &Path, _out: &mut dyn BackupOutputStream, last: Option<&Path>)
              -> Result<(), std::io::Error> {
        let out_file_name = dst.join(src.parent().unwrap()).into_os_string().into_string()
            .map_err(|_os_str| std::io::Error::new(std::io::ErrorKind::InvalidInput,
                                                   "Filename not representable as str."))? +
            "/tsnapshot-" + src.file_name().unwrap().to_str().unwrap() + self.algorithm.extension();
        log::debug!("Compressing {:?} into compressed file {:?}", src, out_file_name);
        log::debug!("Creating output file {:?}...", out_file_name);
        let out_file = std::fs::File::create(out_file_name)?;
        log::debug!("Initializing {:?} compressor...", self.algorithm);
        let mut compressor = self.algorithm.get_writer(out_file);
        {
            log::debug!("Creating tar backup stream...");
            let mut tar_out = TarOutputStream::new(compressor.writer());
            log::debug!("Continuing backup with tar stream...");
            self.config.backup(src, dst, &mut tar_out, last)?;
        }
        return compressor.close();
    }
    
    fn get_subpath(&self) -> &Path { return &self.config.get_subpath(); }
    fn get_subconfig(&self, path: &Path) -> Option<&dyn DirectoryConfig> { return self.config.get_subconfig(path) }
}

#[derive(Debug)]
struct HardLinkedDirectory {
    config:           BasicDirectory,
    max_link_count:   u64,
    detection_method: ChangeDetectionMethod
}

impl HardLinkedDirectory {
    fn new(obj: &json::object::Object) -> Result<Box<dyn DirectoryConfig>, Error> {
        let (config, _) = BasicDirectory::new(obj)?;
        return Ok(Box::new(HardLinkedDirectory {
            config:     config,
            max_link_count:   uint_from_opt_json_prop(obj, "max_link_count", u64::MAX)?,
            detection_method: str_from_opt_json_prop(&obj, "change_detection", "timestamp")?
                .parse::<ChangeDetectionMethod>()?
        }));
    }
}

impl std::str::FromStr for ChangeDetectionMethod {
    type Err = Error;
    
    fn from_str(s: &str) -> Result<ChangeDetectionMethod, Error> {
        match s.to_lowercase().as_str() {
            "timestamp"  => Ok(ChangeDetectionMethod::Timestamp),
            "full"       => Ok(ChangeDetectionMethod::FullCompare),
            method       => Err(ParseError::UnknownOption(method.to_string())),
        }
    }
}

impl DirectoryConfig for HardLinkedDirectory {
    fn backup(&self, src: &Path, dst: &Path, _out: &mut dyn BackupOutputStream, last_opt: Option<&Path>)
              -> Result<(), std::io::Error>  {
        log::debug!("Creating hard linked backup stream...");
        if let Some(last) = last_opt {
            log::debug!("Continuing backup with hard linked stream...");
            let mut hard_link_out = HardLinkOutputStream::new(dst, last, self.max_link_count, &self.detection_method);
            self.config.backup(src, dst, &mut hard_link_out, last_opt)
        }
        else {
            log::debug!("No reference directory.  Continuing backup with copy stream...");
            let mut copy_out = CopyOutputStream::new(dst);
            self.config.backup(src, dst, &mut copy_out, last_opt)
        }
    }
    
    fn get_subpath(&self) -> &Path { return &self.config.get_subpath(); }
    fn get_subconfig(&self, path: &Path) -> Option<&dyn DirectoryConfig> { return self.config.get_subconfig(path); }
}

#[derive(Debug)]
enum Filter {
    Name(Regex),
    Size(u64, u64),
    MimeType(Regex),
    Not(Box<Filter>),
    And(Vec<Filter>),
    Or(Vec<Filter>)
}

impl Filter {
    fn new(json: &json::JsonValue) -> Result<Filter, Error> {
        match json {
            JsonValue::Object(obj) => {
                match str_from_json_prop(&obj, "on")? {
                    "name" => {
                        let regex_str = str_from_json_prop(&obj, "pattern")?;
                        Regex::new(regex_str)
                            .map(|regex| Filter::Name(regex))
                            .map_err(|regex_err| ParseError::BadRegex(regex_str.to_string(), regex_err))
                    },
                    "size" => Ok(Filter::Size(
                        uint_from_opt_json_prop(&obj, "min", 0)?,
                        uint_from_opt_json_prop(&obj, "max", u64::MAX)?
                    )),
                    "mime" => {
                        let regex_str = str_from_json_prop(&obj, "pattern")?;
                        Regex::new(regex_str)
                            .map(|regex| Filter::MimeType(regex))
                            .map_err(|regex_err| ParseError::BadRegex(regex_str.to_string(), regex_err))
                    },
                    "not"  => Ok(Filter::Not(Box::new(
                        Filter::new(obj.get("filter").ok_or(ParseError::RequiredPropMissing("filter"))?)?
                    ))),
                    "and"  => Ok(Filter::And(Filter::new_vec(obj.get("filters"))?)),
                    "or"   => Ok(Filter::Or(Filter::new_vec(obj.get("filters"))?)),
                    other  => Err(ParseError::UnknownOption(other.to_string()))
                }
            },
            _                      => Err(ParseError::NotAnObject("filter"))
        }
    }

    fn new_vec(json: Option<&json::JsonValue>) -> Result<Vec<Filter>, Error> {
        match json {
            None                         => Ok(vec![]),
            Some(JsonValue::Array(vec))  => vec.iter().map(Filter::new).collect(),
            Some(..)                     => Err(ParseError::NotAnArray("filters"))
        }
    }

    fn matches(&self, path: &Path, metadata: &fs::Metadata) -> bool {
        match self {
            Filter::Name(regex)     => path.to_str()
                .map(|path| regex.is_match(path))
                .unwrap_or(false),
            Filter::Size(min, max)  => metadata.len() >= *min && metadata.len() <= *max,
            Filter::MimeType(regex) => mime_guess::from_path(path).first()
                .map(|mime| regex.is_match(mime.essence_str()))
                .unwrap_or(false),
            Filter::Not(filter)     => !filter.matches(path, metadata),
            Filter::And(filters)    => filters.iter().all(|filter| filter.matches(path, metadata)),
            Filter::Or(filters)     => filters.iter().any(|filter| filter.matches(path, metadata))
        }
    }
}

// ----- Utility Functions -----------------------------------------------------

fn log_level_from_str(s: &str) -> Result<log::LevelFilter, Error> {
    match s.to_lowercase().as_str() {
        "silent"  => Ok(log::LevelFilter::Off),
        "debug"   => Ok(log::LevelFilter::Debug),
        "verbose" => Ok(log::LevelFilter::Info),
        "warning" => Ok(log::LevelFilter::Warn),
        "error"   => Ok(log::LevelFilter::Error),
        lvl       => Err(ParseError::UnknownOption(lvl.to_string())),
    }
}

fn uint_from_json_prop<'a>(json: &'a json::object::Object, prop: &'static str) -> Result<u64, Error> {
    return json
        .get(prop).ok_or(ParseError::RequiredPropMissing(prop))?
        .as_u64() .ok_or(ParseError::NotAnUnsignedInt(prop));
}

fn str_from_json_prop<'a>(json: &'a json::object::Object, prop: &'static str) -> Result<&'a str, Error> {
    return json
        .get(prop).ok_or(ParseError::RequiredPropMissing(prop))?
        .as_str() .ok_or(ParseError::NotAString(prop));
}

fn uint_from_opt_json_prop<'a>(json: &'a json::object::Object, prop: &'static str, default: u64) -> Result<u64, Error> {
    return json
        .get(prop)
        .map(|value| value.as_u64().ok_or(ParseError::NotAnUnsignedInt(prop)))
        .unwrap_or(Ok(default));
}

fn str_from_opt_json_prop<'a>(json: &'a json::object::Object, prop: &'static str, default: &'a str) -> Result<&'a str, Error> {
    return json
        .get(prop)
        .map(|value| value.as_str().ok_or(ParseError::NotAString(prop)))
        .unwrap_or(Ok(default));
}
