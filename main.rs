use clap::{self, Parser};
use std::{
    fs::File,
    io::{self, BufReader, Error, Read},
    path::PathBuf,
    process::exit,
    str,
};

#[derive(Parser)]
#[clap(author)]
struct Args {
    /// The JPEG file to parse
    #[clap(num_args = 1..)]
    file: Option<Vec<PathBuf>>,

    /// Display verbose output (e.g specific marker types)
    #[clap(short)]
    verbose: bool,
}

#[derive(Debug, Copy, Clone)]
struct ImgProps {
    width: usize,
    height: usize,
    bit_depth: usize,
    components: usize,
}

#[derive(PartialEq, Eq, Debug)]
enum JpegMarker {
    /// Start of Image (0xD8)
    START,

    /// Indicative of a potential marker. E.g [0xFF, 0xC0] -> [JpegMarker::INDICATOR, JpegMarker::SOF(0xC0)]
    INDICATOR,

    /// Application data. Contains the type of byte (e.g 0xE0 or 0xE1)
    APP(u8),

    /// Start of Frame (0xC0 -> 0xC2). Contains the type of byte (e.g 0xC0)
    SOF(u8),

    /// End of Image (0xD9)
    END,

    /// Not a Marker, contains the byte
    None(u8),
}

impl JpegMarker {
    fn from_u8(marker: u8) -> JpegMarker {
        match marker {
            0xFF => JpegMarker::INDICATOR,
            0xD8 => JpegMarker::START,
            0xD9 => JpegMarker::END,
            0xE0 | 0xE1 | 0xE2 => JpegMarker::APP(marker),
            0xC0..=0xC2 => JpegMarker::SOF(marker),
            _ => JpegMarker::None(marker),
        }
    }
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    if args.file.is_none() {
        eprintln!("Please provide a JPEG image as argument!");
        eprintln!(
            "USAGE: {} <FILENAME.jpeg>",
            std::env::args().next().unwrap()
        );
        exit(1);
    }

    let filenames = args.file.unwrap();
    if args.verbose {
        println!("Attempting to parse {} file(s).", filenames.len());
    }
    for filename in &filenames {
        if let Err(e) = parse_jpeg(filename.to_str().unwrap_or(""), args.verbose) {
            println!("Error: {}", Error::to_string(&e));
        }
    }

    if args.verbose {
        println!("Successfully parsed {} file(s).", filenames.len());
    }
    Ok(())
}

fn parse_jpeg(filename: &str, verbose: bool) -> Result<(), io::Error> {
    if verbose {
        println!("Parsing file {}", filename);
    }

    let file = File::open(filename)?;
    let mut breader = BufReader::new(file);
    let mut buf = [0u8; 8192];
    let mut is_first_read = true;

    let mut sof_segments: Vec<(u8, ImgProps)> = Vec::new();
    let mut skip_bytes = 0usize;
    let mut ident: &str = "";

    while let Ok(amnt) = breader.read(&mut buf) {
        if amnt == 0 {
            break;
        } else if is_first_read && amnt > 1 {
            if JpegMarker::from_u8(buf[1]) != JpegMarker::START {
                return Err(Error::new(
                    io::ErrorKind::InvalidData,
                    filename.to_owned() + " is not a valid JPEG image!",
                ));
            }
            is_first_read = false;
        }

        // Indice count
        let buf_len = buf.len() - 1;
        for idx in 0..buf_len {
            if skip_bytes > 0 {
                skip_bytes -= 1;
                continue;
            }

            let byte = JpegMarker::from_u8(buf[idx]);
            // 0xFF 0x00 is byte stuffing.
            if byte == JpegMarker::INDICATOR {
                match JpegMarker::from_u8(buf[(idx + 1).min(buf_len)]) {
                    JpegMarker::APP(b) => {
                        let size: usize =
                            (buf[(idx + 2).min(buf_len)] + buf[(idx + 3).min(buf_len)] - 2).into();
                        ident = match b {
                            0xE0 => "JFIF",
                            0xE1 => "EXIF",
                            _ => ident,
                        };
                        if verbose {
                            println!("APP Marker - 0x{:X}\nSize of APP Section (excluding initial 0xFF 0x{:X}): {} bytes", b, b, size);
                            print!("NULL Terminated String: ");
                            let mut idx = idx + 4;
                            while buf[idx] != 0 {
                                print!("{}", char::from_u32(buf[idx] as u32).unwrap());
                                idx += 1;
                                skip_bytes += 1;
                            }
                            println!("")
                        }
                    }
                    JpegMarker::SOF(b) => {
                        let size: usize =
                            (buf[(idx + 2).min(buf_len)] + buf[(idx + 3).min(buf_len)] - 2).into();

                        let mut start_frame = Vec::new();
                        start_frame.extend(buf[idx + 4..(idx + 4 + size)].iter().cloned());

                        let parsed_frame = parse_start_frame(start_frame);
                        sof_segments.push((b, parsed_frame.clone()));

                        if verbose {
                            println!("SOF Marker - 0x{:X}\nSize of SOF Section (excluding initial 0xFF 0x{:X}): {} bytes. Frame was {:?}", b, b, size, parsed_frame)
                        }
                    }
                    _ => continue,
                }
            }
        }
    }

    print!("File ({}) ", filename);
    print!("[{}] ", ident);
    sof_segments.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let parsed_frame = &sof_segments.last().unwrap().1;
    print!(
        "{}x{} Bit Depth {}, Components {}",
        parsed_frame.width, parsed_frame.height, parsed_frame.bit_depth, parsed_frame.components
    );
    println!("");

    Ok(())
}

fn parse_start_frame(frame: Vec<u8>) -> ImgProps {
    let mut frame = frame.into_iter();
    // Skip the first byte
    let bit_depth = frame.next().unwrap() as usize;

    let mut conv = || -> usize {
        vec![frame.next().unwrap(), frame.next().unwrap()]
            .iter()
            .fold(0, |acc, v| {
                if acc == 0 {
                    (*v as usize) << 8
                } else {
                    acc + *v as usize
                }
            })
    };
    ImgProps {
        bit_depth,
        height: conv(),
        width: conv(),
        components: frame.next().unwrap() as usize,
    }
}
