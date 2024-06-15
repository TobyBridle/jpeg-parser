use clap::{self, Parser};
use std::{
    any::Any,
    fs::File,
    io::{self, BufRead, BufReader, Error, Read},
    mem::transmute,
    path::PathBuf,
    process::exit,
    str,
    thread::sleep,
    time::Duration,
};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Parser)]
#[clap(author)]
struct Args {
    /// The JPEG file to parse
    #[clap(num_args = 1..)]
    file: Option<Vec<PathBuf>>,
}

#[derive(Debug)]
struct MarkerTracking {
    start: bool,
    application_data: Vec<u8>,
    start_frame: Vec<u8>,
    end: bool,
}

impl MarkerTracking {
    fn new() -> Self {
        MarkerTracking {
            start: false,
            application_data: vec![],
            start_frame: vec![],
            end: false,
        }
    }
}

const MARKER_INITIAL: u8 = 0xFF;
const MARKER_START: u8 = 0xD8;
const MARKER_APPLICATION_DATA_E0: u8 = 0xE0;
const MARKER_APPLICATION_DATA_E1: u8 = 0xE1;
const MARKER_START_OF_FRAME: u8 = 0xC0;
const MARKER_END: u8 = 0xD9;

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
    for filename in filenames {
        if let Ok(mut file) = File::open(filename.clone()) {
            let mut tracking = MarkerTracking::new();
            let mut buf = vec![];
            let byte_count = file.read_to_end(&mut buf);
            if byte_count.is_err() {
                eprintln!("Could not read {:?}.", filename.clone());
                exit(1);
            }
            let byte_count = byte_count.unwrap();
            let mut skip_bytes = 0;
            for idx in 0..byte_count {
                if skip_bytes > 0 {
                    skip_bytes -= 1;
                    continue;
                }
                if buf[idx] == MARKER_INITIAL {
                    match buf[(idx + 1).min(byte_count)] {
                        MARKER_START => {
                            tracking.start = true;
                            skip_bytes += 1;
                        }
                        MARKER_APPLICATION_DATA_E0 | MARKER_APPLICATION_DATA_E1 => {
                            // Get size of application data (in bytes)
                            let size: usize = (buf[(idx + 2).min(byte_count)]
                                + buf[(idx + 3).min(byte_count)]
                                - 2)
                            .into();
                            // + 4 comes from the +2 of Markers and +2 of bytes signifying size
                            tracking
                                .application_data
                                .extend(buf[idx + 4..(idx + 4 + size)].iter().cloned());
                            skip_bytes += size + 2;
                        }
                        MARKER_START_OF_FRAME | 0xC1 | 0xC2 => {
                            // if !tracking.start_frame.is_empty() {
                            //     continue;
                            // }
                            // Get size of application data (in bytes)
                            let size: usize = (buf[(idx + 2).min(byte_count)]
                                + buf[(idx + 3).min(byte_count)]
                                - 2)
                            .into();
                            // + 4 comes from the +2 of Markers and +2 of bytes signifying size
                            tracking.start_frame = Vec::new();
                            tracking
                                .start_frame
                                .extend(buf[idx + 4..(idx + 4 + size)].iter().cloned());
                            skip_bytes += size + 2;
                        }
                        MARKER_END => {
                            tracking.end = true;
                            skip_bytes += 1;
                        }
                        _ => {}
                    }
                }
            }
            if !tracking.start {
                eprintln!("Expected JPEG File input!");
                exit(1);
            }

            let parsed_frame = parse_start_frame(tracking.start_frame);
            print!(
                "File ({}) ",
                filename.file_name().unwrap().to_str().unwrap()
            );
            print!(
                "{} ",
                str::from_utf8(
                    &tracking
                        .application_data
                        .take(4)
                        .bytes()
                        .map(|x| x.unwrap())
                        .collect::<Vec<u8>>()
                )
                .unwrap()
                .to_uppercase()
            );
            print!("{}x{}", parsed_frame.width, parsed_frame.height);
            println!("")
        } else {
            println!("We did not manage to open the file!")
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ImgProps {
    width: usize,
    height: usize,
    bit_depth: usize,
    components: usize,
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
