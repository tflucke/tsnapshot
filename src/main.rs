// ----- Set up crate ----------------------------------------------------------

#[macro_use]
extern crate static_assertions;

mod config;
mod backup;
mod catalog;

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
    let config = match std::fs::read_to_string(&args[1]) {
        Ok(file_contents) => {
            match crate::config::Configuration::new(file_contents.as_str()) {
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
    let mut catalog = {
        let catalog_file = match std::fs::OpenOptions::new()
            .read(true)
            .create(true)
            .open(&catalog_file_name) {
                Ok(file) => file,
                Err(err) => {
                    log::error!("Failed to open catalog file {:?} for reading: {:?}.",
                             catalog_file_name, err);
                    return 1
                }
            };
        match crate::catalog::BackupCatalog::new(catalog_file) {
            Ok(file) => file,
            Err(err) => {
                log::error!("Failed to parse catalog file {:?}: {:?}.", catalog_file_name, err);
                return 1
            }
        }
    };
    match config.backup(catalog.most_recent()) {
        Ok(new_dir)   => catalog.push(&new_dir),
        Err(err)      => {
            log::error!("Failed to backup due to error: {:?}.", err);
            return 1
        }
    }
    match catalog.clean(config.keep_limit) {
        Ok(_) => (),
        Err(err) => {
            log::error!("Failed to clean backups: {:?}.", err);
            return 1
        }
    };
    let catalog_file = match std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(&catalog_file_name) {
            Ok(file) => file,
            Err(err) => {
                log::error!("Failed to open catalog file {:?} for writing: {:?}.",
                         catalog_file_name, err);
                return 1
            }
        };
    match catalog.save_to(catalog_file) {
        Ok(_) => (),
        Err(err) => {
            log::error!("Failed to save new catalog file: {:?}.", err);
            return 1
        }
    };
    return 0
}

// ----- Entry Point -----------------------------------------------------------

fn main() {
    std::process::exit(real_main(std::env::args().collect()));
}
