use std::{
    io::{
        self,
        Error,
        ErrorKind,
        Read,
    },
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    str,
};

use termcolor::Buffer;
use asciify::AsciiBuilder;
use clap::Parser;
use image::ImageBuffer;

fn get_video_dimensions(file_path: &Path) -> io::Result<(u32, u32)> {
    // Use the ffprobe command to get video information
    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            &format!("{}", file_path.display()),
        ])
        .output()?;

    if !output.status.success() {
        return Err(Error::new(ErrorKind::Other, "Error executing ffprobe"));
    }

    // Convert the output to a String
    let output_str = str::from_utf8(&output.stdout)
        .map_err(|_| {
            Error::new(ErrorKind::Other, "Error converting output to String")
        })
        .unwrap(); //?;

    // Parse the output
    let v: Vec<&str> = output_str.trim().split(',').collect();

    if v.len() != 2 {
        return Err(Error::new(
            ErrorKind::Other,
            "Invalid file or video channel",
        ));
    }

    let width = v[0]
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorKind::Other, "Error parsing width"))
        .unwrap(); //?;
    let height = v[1]
        .parse::<u32>()
        .map_err(|_| Error::new(ErrorKind::Other, "Error parsing height"))?;

    Ok((width, height))
}

fn get_video_fps(video_file: &Path) -> io::Result<f64> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-select_streams", "v:0",
            "-show_entries", "stream=r_frame_rate",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &format!("{}", video_file.display()),
        ])
        .output()?;

    if !output.status.success() {
        return Err(io::Error::new(io::ErrorKind::Other, "ffprobe command failed"));
    }

    let frame_rate_str = String::from_utf8_lossy(dbg!(&output.stdout));
    let mut frame_rate = frame_rate_str.trim().split("/");
    let numerator = frame_rate.next().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to parse frame rate"))?.parse::<f64>()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to parse frame rate"))?;
    let denominator = frame_rate.next().ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to parse frame rate"))?.parse::<f64>()
        .map_err(|_| io::Error::new(io::ErrorKind::Other, "failed to parse frame rate"))?;

    Ok(numerator / denominator)
}

fn process_video_file<F, T>(
    video_file: &PathBuf,
    target_width: u32,
    target_height: u32,
    writer: &mut T,
    mut handle_output: F,
) -> io::Result<()>
where
    F: FnMut(&mut T, &mut std::process::ChildStdout) -> io::Result<()>
{
    let mut child = Command::new("ffmpeg")
        .arg("-hide_banner")
        .arg("-i")
        .arg(&format!("{}", video_file.display()))
        .arg("-vf")
        .arg(&format!("scale={}:{}", target_width, target_height))
        .arg("-f")
        .arg("rawvideo")
        .arg("-pix_fmt")
        .arg("bgra")
        .arg("-")
        .stdout(std::process::Stdio::piped())
        .spawn()?;

    let retval;
    if let Some(ref mut stdout) = child.stdout {
        retval = handle_output(writer, stdout)?;
    }

    else {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "cannot produce stdout",
        ));
    }

    let ecode = child.wait()?;

    if !ecode.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "ffmpeg command failed",
        ));
    }

    Ok(retval)
}

fn process_video_stream<T>(
    target_width: u32,
    target_height: u32,
    mut stream: impl Read,
    writer: &mut T, 
    mut per_file: impl FnMut(&mut T, &Buffer),
) -> io::Result<()> {
    let frame_pixels = target_width * target_height;
    let frame_bytes = (frame_pixels * 4) as usize;

    let mut bytes = vec![0u8; frame_bytes];
    let mut output_buffer = Buffer::ansi();
    while {
        // TODO: fix this so this doesn't read too fast that we catch up to
        // FFMpeg's output and make it fail fast
        match stream.read(&mut bytes).unwrap() {
            x if x == frame_bytes => true,
            0 => false,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected eof",
                ))
            },
        }
    } {
        let image =
            ImageBuffer::from_raw(target_width, target_height, bytes).unwrap();
        let image = image::DynamicImage::ImageBgra8(image);

        output_buffer.clear();
        AsciiBuilder::new_from_image(image)
            .set_deep(true) // what if you used false?
            .to_stream_colored(&mut output_buffer);
        per_file(writer, &output_buffer);

        bytes = vec![0u8; frame_bytes];
    }

    Ok(())
}

fn new_target_dimensions(
    src_width: u32,
    src_height: u32,
    char_width: u32,
    char_height: u32,
    dest_width: Option<u32>,
    dest_height: Option<u32>,
) -> (u32, u32) {
    match (dest_width, dest_height) {
        // if both width and height are given, they take priority
        (Some(w), Some(h)) => (w, h), 

        // if only width is given, it gets priority and height gets derived for
        // both aspect ratio and char ratio
        (Some(w), None) => (
            w,
            src_height * w * char_height / (src_width * char_width)
        ),
        (None, Some(h)) => (
            src_width * h * char_height / (src_height * char_width),
            h
        ),
        (None, None) => unreachable!(),
    }
}

fn get_char_dims(char_string: Option<String>) -> Result<(u32, u32), &'static str> {
    let char_string = match char_string {
        None => return Ok((1, 1)),
        Some(x) => x,
    };

    let mut iter = char_string.split("x");
    let width = iter
        .next()
        .ok_or_else(|| "string is empty")?
        .parse::<u32>()
        .map_err(|_| "cannot convert to u32")?;
    let height = iter
        .next()
        .ok_or_else(|| "string is empty")?
        .parse::<u32>()
        .map_err(|_| "input doesn't have height")?;
    
    Ok((width, height))
}

#[derive(Debug, Default)]
struct MovieInProgress {
    starting: String,
    current: String,
    frame_diffs: Vec<Vec<diff::Result<char>>>,
}

#[derive(Parser)]
pub struct Args {
    video: PathBuf,
    #[clap(long)]
    target_width: Option<u32>,
    #[clap(long)]
    target_height: Option<u32>,
    #[clap(long)]
    char_dims: Option<String>,
}

fn main() {
    use std::io::Write as _;

    let args = Args::parse();

    if args.target_width == Some(0) {
        panic!("target_width cannot be zero");
    }

    if args.target_height == Some(0) {
        panic!("target_height cannot be zero");
    }

    if args.target_width.is_none() && args.target_height.is_none() {
        panic!("must set either target_width or target_height")
    }

    let (char_width, char_height) = get_char_dims(args.char_dims).unwrap();

    let (width, height) = get_video_dimensions(&args.video).unwrap();
    let (target_width, target_height) = new_target_dimensions(
        width,
        height,
        char_width, char_height,
        args.target_width,
        args.target_height,
    );

    let framerate = get_video_fps(&args.video).unwrap();
    dbg!(&framerate);

    let mut encoder = lz4::EncoderBuilder::new().level(9).build(std::io::stdout().lock()).unwrap();

    writeln!(&mut encoder, "{}", framerate);
    writeln!(&mut encoder, "{} {}", target_width, target_height);

    let per_string = move |encoder: &mut lz4::Encoder<_>, s: &Buffer| {
        encoder.write(s.as_slice());
    };

    let movie = process_video_file(&args.video, target_width, target_height, &mut encoder, |w, r| {
        process_video_stream(target_width, target_height, r, w, per_string)
    })
    .unwrap();

    // format:
    // - framerate
    // - dimensions
    // - audio (TODO)
    // - video

    encoder.flush();
}
