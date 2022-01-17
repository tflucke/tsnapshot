// ----- Set up crate ----------------------------------------------------------

extern crate argparse;

use std::path::Path;
use std::fs;
use tsnapshot::compression::*;
use std::fs::OpenOptions;

// ----- Logging Data Structures -----------------------------------------------

static MY_LOGGER: MyLogger = MyLogger;

struct MyLogger;

impl log::Log for MyLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool { true }

    fn log(&self, record: &log::Record) {
        let now = chrono::Local::now();
        if record.level() >= log::Level::Debug {
            println!("[{}] {} <{}:{}>: {}",
                     record.level(),
                     now.format("%b %-d, %-I:%M:%S").to_string(),
                     record.file().unwrap_or("???"),
                     record.line().unwrap_or(0),
                     record.args());
        }
        else {
            println!("[{}] {}: {}", record.level(), now.format("%b %-d, %-I:%M:%S").to_string(), record.args());
        }
    }

    fn flush(&self) {}
}

// ----- Main ------------------------------------------------------------------

fn real_main(args: Vec<String>) -> i32 {
    let config = match fs::read_to_string(&args[1]) {
        Ok(file_contents) => {
            match tsnapshot::config::Configuration::new(file_contents.as_str()) {
                Ok(res) => res,
                Err(parse_err) => {
                    println!("{:?}", parse_err);
                    return 1
                }
            }
        }
        Err(io_err) => {
            println!("{:?}", io_err);
            return 1
        }
    };
    match log::set_logger(&MY_LOGGER).map(|()| log::set_max_level(config.verbosity)) {
        Ok(())   => (),
        Err(err) => {
            println!("Logging failed to initialize: {:?}.", err);
            return 1
        }
    };
    let catalog_file_name = config.destination_dir.join("catalog.txt");
    let catalog = {
        if let Some(catalog_file) = match OpenOptions::new()
            .read(true)
            .open(&catalog_file_name) {
                Ok(file)                                               => Some(file),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
                Err(err)                                               => {
                    log::error!("Failed to open catalog file {:?} for reading: {:?}.",
                             catalog_file_name, err);
                    return 1
                }
            }
        {
            match tsnapshot::catalog::BackupCatalog::new(catalog_file) {
                Ok(catalog) => catalog,
                Err(err) => {
                    log::error!("Failed to parse catalog file {:?}: {:?}.", catalog_file_name, err);
                    return 1
                }
            }
        }
        else {
            tsnapshot::catalog::BackupCatalog::empty()
        }
    };
    catalog.most_recent().map(
        |src| match extract(&src, &src, Path::new(&args[2])) {
            Ok(_) => (),
            Err(err) => log::error!("Failed to restore backup: {:?}", err)
        }
    );
    return 0
}

// ----- Extraction Logic ------------------------------------------------------

fn extract(src_path: &Path, rel_path: &Path, dst_path: &Path) -> Result<(), std::io::Error> {
    if let Some(name_os) = src_path.file_name() {
        if let Some(name_str) = name_os.to_str() {
            if name_str.starts_with("tsnapshot-") {
                if let Some(compression) = CompressionAlgorithm::from_extension(name_str) {
                    log::info!("Extracting {:?} into {:?}", src_path, dst_path);
                    return compression.decode_file(src_path, dst_path)
                }
                else {
                    log::debug!("{:?} is not a recognized extension", name_str);
                }
            }
        }
    }
    else {
        log::debug!("{:?} does not have an extension", src_path);
    }
    log::debug!("{:?} {:?} {:?}", src_path, rel_path, dst_path);
    let src_meta = metadata(src_path)?;
    let extraction_dst = dst_path.join(src_path.strip_prefix(rel_path).unwrap());
    if src_meta.is_dir() {
        log::info!("Creating {:?}", extraction_dst);
        match fs::create_dir(extraction_dst) {
            Ok(_)                                                       => (),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(err)                                                    => return Err(err)
        };
        for entry_res in fs::read_dir(src_path)? {
            let entry_path = &entry_res?.path();
            extract(entry_path, rel_path, dst_path)?;
        };
        Ok(())
    }
    else {
        log::info!("Copying {:?} into {:?}", src_path, extraction_dst);
        fs::copy(src_path, extraction_dst)?;
        Ok(())
    }
}

// ----- Temporary Utility Functions -------------------------------------------

/* [tflucke] 2022-01-16: Rust uses statx to get path metadata, but fakechroot
 * cannot wrap it yet.  Until that is supported, we need to open the file
 * to get any metadata during a test.
 */
#[cfg(debug_assertions)]
#[inline]
fn metadata(path: &Path) -> Result<fs::Metadata, std::io::Error> {
    return OpenOptions::new().read(true).open(path)?.metadata();
}

#[cfg(not(debug_assertions))]
#[inline]
fn metadata(path: &Path) -> Result<fs::Metadata, std::io::Error> {
    return path.metadata();
}
    
// ----- Entry Point -----------------------------------------------------------

fn main() {
    std::process::exit(real_main(std::env::args().collect()));
}
