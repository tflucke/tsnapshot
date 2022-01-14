use std::path::Path;

#[derive(Debug)]
pub enum CompressionAlgorithm {
    Bzip2(CompressionLevel),
    Gzip(CompressionLevel),
    Zip
}

impl CompressionAlgorithm {
    pub fn from_extension(string: &str) -> Option<CompressionAlgorithm> {
        if string.ends_with(".tar.bz2") {
            Some(CompressionAlgorithm::Bzip2(CompressionLevel::Fast))
        }
        else if string.ends_with(".tar.gz") {
            Some(CompressionAlgorithm::Gzip(CompressionLevel::Fast))
        }
        else if string.ends_with(".zip") {
            Some(CompressionAlgorithm::Zip)
        }
        else {
            None
        }
    }
    
    pub fn extension(&self) -> &'static str {
        match self {
            CompressionAlgorithm::Bzip2(..) => ".tar.bz2",
            CompressionAlgorithm::Gzip(..)  => ".tar.gz",
            CompressionAlgorithm::Zip       => ".zip"
        }
    }
    
    pub fn get_writer(&self, out_writer: std::fs::File) -> Box<dyn Compressor> {
        match self {
            CompressionAlgorithm::Bzip2(level) => {
                use bzip2::write::*;

                struct Bzip2Compressor {
                    encoder: BzEncoder<std::fs::File>
                }
                impl Compressor for Bzip2Compressor {
                    fn writer(&mut self) -> &mut dyn std::io::Write {
                        &mut self.encoder
                    }
                    fn close(&mut self) -> Result<(), std::io::Error> {
                        self.encoder.try_finish()
                    }
                }
                Box::new(Bzip2Compressor {
                    encoder: BzEncoder::new(out_writer, match level {
                        CompressionLevel::Best       => bzip2::Compression::best(),
                        CompressionLevel::Fast       => bzip2::Compression::fast(),
                        CompressionLevel::Level(lvl) => bzip2::Compression::new((*lvl).into())
                    }),
                })
            },
            CompressionAlgorithm::Gzip(level)  => {
                use flate2::write::*;

                struct GzipCompressor {
                    encoder: GzEncoder<std::fs::File>
                }
                impl Compressor for GzipCompressor {
                    fn writer(&mut self) -> &mut dyn std::io::Write {
                        &mut self.encoder
                    }
                    fn close(&mut self) -> Result<(), std::io::Error> {
                        self.encoder.try_finish()
                    }
                }
                Box::new(GzipCompressor {
                    encoder: GzEncoder::new(out_writer, match level {
                        CompressionLevel::Best       => flate2::Compression::best(),
                        CompressionLevel::Fast       => flate2::Compression::fast(),
                        CompressionLevel::Level(lvl) => flate2::Compression::new((*lvl).into())
                    }),
                })
            },
            CompressionAlgorithm::Zip  => todo!(),
        }
    }
    
    pub fn decode_file(&self, src_path: &Path, dst_path: &Path) -> Result<(), std::io::Error> {
        let file = std::fs::File::open(src_path)?;
        use tar::Archive;
        match self {
            CompressionAlgorithm::Bzip2(..) => {
                use bzip2::read::*;
                let tar = BzDecoder::new(file);
                let mut archive = Archive::new(tar);
                archive.unpack(dst_path)
            },
            CompressionAlgorithm::Gzip(..)  => {
                use flate2::read::*;
                let tar = GzDecoder::new(file);
                let mut archive = Archive::new(tar);
                archive.unpack(dst_path)
            },
            CompressionAlgorithm::Zip       => todo!()
        }
    }
}

pub trait Compressor {
    fn writer(&mut self) -> &mut dyn std::io::Write;
    fn close(&mut self) -> Result<(), std::io::Error>;
}

impl std::str::FromStr for CompressionAlgorithm {
    type Err = crate::config::Error;

    fn from_str(s: &str) -> Result<CompressionAlgorithm, crate::config::Error> {
        match s.to_lowercase().as_str() {
            "bzip2"   => Ok(CompressionAlgorithm::Bzip2(CompressionLevel::Fast)),
            "gzip"    => Ok(CompressionAlgorithm::Gzip(CompressionLevel::Fast)),
            "zip"     => Ok(CompressionAlgorithm::Zip),
            algorithm => Err(crate::config::ParseError::UnknownOption(algorithm.to_string())),
        }
    }
}

#[derive(Debug)]
pub enum CompressionLevel {
    Fast,
    Best,
    Level(u8)
}

impl std::str::FromStr for CompressionLevel {
    type Err = crate::config::Error;

    fn from_str(s: &str) -> Result<CompressionLevel, crate::config::Error> {
        match s.to_lowercase().as_str() {
            "best" => Ok(CompressionLevel::Best),
            "fast" => Ok(CompressionLevel::Fast),
            level_str if level_str.len() == 1 && level_str.chars().nth(0).unwrap().is_numeric() => {
                Ok(CompressionLevel::Level(level_str.parse::<u8>().unwrap()))
            },
            level  => Err(crate::config::ParseError::UnknownOption(level.to_string())),
        }
    }
}
