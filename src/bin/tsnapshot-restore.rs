// ----- Set up crate ----------------------------------------------------------

use std::path::Path;
use std::fs;
use tsnapshot::compression::*;

// ----- Main ------------------------------------------------------------------

fn real_main(args: Vec<String>) -> i32 {
    let src_path = Path::new(&args[1]);
    let dst_dir = Path::new(&args[2]);
    match extract(src_path, src_path, dst_dir) {
        Ok(_) => (),
        Err(err) => println!("Failed to restore backup: {:?}", err)
    };
    return 0
}

// ----- Extraction Logic ------------------------------------------------------

fn extract(src_path: &Path, rel_path: &Path, dst_path: &Path) -> Result<(), std::io::Error> {
    if let Some(name_os) = src_path.file_name() {
        if let Some(name_str) = name_os.to_str() {
            if name_str.starts_with("tsnapshot-") {
                if let Some(compression) = CompressionAlgorithm::from_extension(name_str) {
                    println!("Extracting {:?} into {:?}", src_path, dst_path);
                    return compression.decode_file(src_path, dst_path)
                }
                else {
                    println!("{:?} is not a recognized extension", name_str);
                }
            }
        }
    }
    else {
        println!("{:?} does not have an extension", src_path);
    }
    println!("{:?} {:?} {:?}", src_path, rel_path, dst_path);
    let extraction_dst = dst_path.join(src_path.strip_prefix(rel_path).unwrap());
    if src_path.is_dir() {
        println!("Creating {:?}", extraction_dst);
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
        println!("Copying {:?} into {:?}", src_path, extraction_dst);
        fs::copy(src_path, extraction_dst)?;
        Ok(())
    }
}

// ----- Entry Point -----------------------------------------------------------

fn main() {
    std::process::exit(real_main(std::env::args().collect()));
}
