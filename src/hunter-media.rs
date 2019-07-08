// Based on https://github.com/jD91mZM2/termplay
// MIT License

use image::{FilterType, DynamicImage, GenericImageView};
use sixel::encoder::Encoder;
use base64;

use termion::color::{Bg, Fg, Rgb};
#[cfg(feature = "video")]
use termion::input::TermRead;


#[cfg(feature = "video")]
use gstreamer::{self, prelude::*};
#[cfg(feature = "video")]
use gstreamer_app;

use failure::Error;
#[cfg(feature = "video")]
use failure::format_err;


use std::io::Write;
#[cfg(feature = "video")]
use std::sync::{Arc, RwLock};

pub type MResult<T> = Result<T, Error>;

fn main() -> MResult<()> {
    let args = std::env::args().collect::<Vec<String>>();
    let xsize: usize = args.get(1)
        .expect("Provide xsize")
        .parse::<usize>()
        .unwrap();
    let ysize = args.get(2)
        .expect("provide ysize")
        .parse()
        .unwrap();
    let xpix = args.get(3)
        .expect("provide xsize in pixels")
        .parse::<usize>()
        .unwrap();
    let ypix = args.get(4)
        .expect("provide ysize in pixels")
        .parse::<usize>()
        .unwrap();
    let preview_type = args.get(5)
        .expect("Provide preview type")
        .parse::<String>()
        .unwrap();
    // #[cfg(feature = "video")]
    let autoplay = args.get(6)
        .expect("Autoplay?")
        .parse::<bool>()
        .unwrap();
    // #[cfg(feature = "video")]
    let mute = args.get(7)
        .expect("Muted?")
        .parse::<bool>()
        .unwrap();
    let sixel = args.get(8)
        .expect("Use SIXEL?")
        .parse::<bool>()
        .unwrap();
    let path = args.get(9).expect("Provide path");

    let target = if sixel {
        if std::env::var("TERM") == Ok(String::from("xterm-kitty")) {
            RenderTarget::Kitty
        } else {
            RenderTarget::Sixel
        }
    } else {
        RenderTarget::Unicode
    };


    let renderer = Renderer::new(target,
                                 xsize,
                                 ysize,
                                 xpix,
                                 ypix);

    let result =
        match preview_type.as_ref() {
            #[cfg(feature = "video")]
            "video" => video_preview(path,
                                     renderer,
                                     autoplay,
                                     mute),

            "image" => image_preview(path,
                                     renderer),

            #[cfg(feature = "video")]
            "audio" => audio_preview(path,
                                     autoplay,
                                     mute),

            #[cfg(feature = "video")]
            _ => { panic!("Available types: video/image/audio") }

            #[cfg(not(feature = "video"))]
            _ => { panic!("Available type: image") }
        };

    if result.is_err() {
        println!("{:?}", &result);
        result
    } else {
        Ok(())
    }
}

fn image_preview(path: &str,
                 renderer: Renderer) -> MResult<()> {
    let img = image::open(&path)?;

    renderer.send_image(&img)?;

    Ok(())
}

#[cfg(feature = "video")]
fn video_preview(path: &String,
                 renderer: Renderer,
                 autoplay: bool,
                 mute: bool)
                 -> MResult<()> {
    let low_fps = renderer.target == RenderTarget::Sixel;

    let (player, appsink) = make_gstreamer(low_fps)?;

    let uri = format!("file://{}", &path);

    player.set_property("uri", &uri)?;


    let renderer = Arc::new(RwLock::new(renderer));
    let crenderer = renderer.clone();




    let p = player.clone();

    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::new()
            .new_sample({
                move |sink| {
                    let sample = match sink.pull_sample() {
                        Some(sample) => sample,
                        None => return gstreamer::FlowReturn::Eos,
                    };

                    let position = p.query_position::<gstreamer::ClockTime>()
                        .map(|p| p.seconds().unwrap_or(0))
                        .unwrap_or(0);

                    let duration = p.query_duration::<gstreamer::ClockTime>()
                        .map(|d| d.seconds().unwrap_or(0))
                        .unwrap_or(0);

                    let renderer = crenderer.clone();
                    std::thread::spawn(move || {
                        renderer.write()
                            .map(|mut r| r.send_frame(&*sample,
                                                      position,
                                                      duration)).ok()
                    });

                    if autoplay == false {
                        // Just render first frame to get a static image
                        match p.set_state(gstreamer::State::Paused)
                            .into_result() {
                                Ok(_) => gstreamer::FlowReturn::Eos,
                                Err(_) => gstreamer::FlowReturn::Error
                            }
                    } else {
                        gstreamer::FlowReturn::Ok
                    }
                }
            })
            .eos({
                move |_| {
                    std::process::exit(0);
                }
            })
            .build()
    );

    if mute == true  || autoplay == false {
        player.set_property("volume", &0.0)?;
    }
    player.set_state(gstreamer::State::Playing).into_result()?;





    read_keys(player, Some(renderer))?;

    Ok(())
}

#[cfg(feature = "video")]
fn read_keys(player: gstreamer::Element,
             renderer: Option<Arc<RwLock<Renderer>>>) -> MResult<()> {
    let seek_time = gstreamer::ClockTime::from_seconds(5);

    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    loop {
        let input = stdin
            .read_line()?
            .unwrap_or_else(|| String::from("q"));


        match input.as_str() {
            "q" => std::process::exit(0),
            ">" => {
                if let Some(mut time) = player
                    .query_position::<gstreamer::ClockTime>() {
                        time += seek_time;

                        player.seek_simple(
                            gstreamer::SeekFlags::FLUSH,
                            gstreamer::format::GenericFormattedValue::from_time(time)
                        )?;
                    }
            },
            "<" => {
                if let Some(mut time) = player
                    .query_position::<gstreamer::ClockTime>() {
                        if time >= seek_time {
                            time -= seek_time;
                        } else {
                            time = gstreamer::ClockTime(Some(0));
                        }

                        player.seek_simple(
                            gstreamer::SeekFlags::FLUSH,
                            gstreamer::format::GenericFormattedValue::from_time(time)
                        )?;
                    }
            }
            "p" => {
                player.set_state(gstreamer::State::Playing).into_result()?;

                // To actually start playing again
                if let Some(time) = player
                    .query_position::<gstreamer::ClockTime>() {
                        player.seek_simple(
                            gstreamer::SeekFlags::FLUSH,
                            gstreamer::format::GenericFormattedValue::from_time(time)
                        )?;
                    }
            }
            "a" => {
                player.set_state(gstreamer::State::Paused).into_result()?;
            }
            "m" => {
                player.set_property("volume", &0.0)?;
            }
            "u" => {
                player.set_property("volume", &1.0)?;
            }
            // TODO add pixel size
            "xy" => {
                if let Some(ref renderer) = renderer {
                    let xsize = stdin.read_line()?;
                    let ysize = stdin.read_line()?;

                    let xsize = xsize.unwrap_or(String::from("0")).parse::<usize>()?;
                    let ysize = ysize.unwrap_or(String::from("0")).parse::<usize>()?;

                    let mut renderer = renderer
                        .write()
                        .map_err(|_| format_err!("Renderer RwLock failed!"))?;

                    renderer.set_size(xsize, ysize)?;
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "video")]
pub fn audio_preview(path: &String,
                     autoplay: bool,
                     mute: bool)
                     -> MResult<()> {
    let (player, _) = make_gstreamer(false)?;

    let uri = format!("file://{}", &path);

    player.set_property("uri", &uri)?;
    let p = player.clone();

    // Since events don't work with audio files...
    std::thread::spawn(move || -> MResult<()> {
        let mut last_pos = None;
        let sleep_duration = std::time::Duration::from_millis(50);
        let mut stdout = std::io::stdout();
        loop {
            std::thread::sleep(sleep_duration);

            let position = p.query_position::<gstreamer::ClockTime>()
                .map(|p| p.seconds().unwrap_or(0))
                .unwrap_or(0);

            let duration = p.query_duration::<gstreamer::ClockTime>()
                .map(|d| d.seconds().unwrap_or(0))
                .unwrap_or(0);

            // Just redo loop until position changes
            if last_pos == Some(position) {
                continue
            }

            last_pos = Some(position);

            // MediaView needs empty line as separator
            writeln!(stdout, "")?;
            // Send height, position and duration
            writeln!(stdout, "0")?;
            writeln!(stdout, "{}", position)?;
            writeln!(stdout, "{}", duration)?;
            stdout.flush()?;
        }

    });

    if mute == true || autoplay == false{
        player.set_property("volume", &0.0)?;
    } else {
        player.set_state(gstreamer::State::Playing).into_result()?;
    }

    read_keys(player, None)?;

    Ok(())
}

#[cfg(feature = "video")]
pub fn make_gstreamer(low_fps: bool) -> MResult<(gstreamer::Element,
                                                 gstreamer_app::AppSink)> {
    gstreamer::init()?;

    let player = gstreamer::ElementFactory::make("playbin", None)
        .ok_or(format_err!("Can't create playbin"))?;

    let videorate = gstreamer::ElementFactory::make("videorate", None)
        .ok_or(format_err!("Can't create videorate element"))?;

    let pnmenc = gstreamer::ElementFactory::make("pnmenc", None)
        .ok_or(format_err!("Can't create PNM-encoder"))?;

    let sink = gstreamer::ElementFactory::make("appsink", None)
        .ok_or(format_err!("Can't create appsink"))?;

    let appsink = sink.clone()
        .downcast::<gstreamer_app::AppSink>()
        .unwrap();


    if low_fps {
        videorate.set_property("max-rate", &10)?;
    } else {
        videorate.set_property("max-rate", &30)?;
    }

    let elems = &[&videorate, &pnmenc, &sink];

    let bin = gstreamer::Bin::new(None);
    bin.add_many(elems)?;
    gstreamer::Element::link_many(elems)?;

    // make input for bin point to first element
    let sink = elems[0].get_static_pad("sink").unwrap();
    let ghost = gstreamer::GhostPad::new("sink", &sink)
        .ok_or(format_err!("Can't create GhostPad"))?;

    ghost.set_active(true)?;
    bin.add_pad(&ghost)?;

    player.set_property("video-sink", &bin.upcast::<gstreamer::Element>())?;

    Ok((player, appsink))
}

#[derive(PartialEq)]
enum RenderTarget {
    Unicode,
    Sixel,
    Kitty
}

struct Renderer {
    // encoder: RwLock<Encoder>,
    target: RenderTarget,
    xsize: usize,
    ysize: usize,
    xsize_pix: usize,
    ysize_pix: usize,
    #[cfg(feature = "video")]
    last_frame: Option<DynamicImage>,
    #[cfg(feature = "video")]
    position: Option<usize>,
    #[cfg(feature = "video")]
    duration: Option<usize>
}

impl Renderer {
    fn new(target: RenderTarget,
           xsize: usize,
           ysize: usize,
           mut xsize_pix: usize,
           mut ysize_pix: usize) -> Renderer {

        if std::env::var("TERM") == Ok(String::from("xterm"))
            && target == RenderTarget::Sixel {
            // xterm has a hard limit on graphics size
            // maybe splitting the image into parts would work?
            if xsize_pix > 1000 { xsize_pix = 1000 };
            if ysize_pix > 1000 { ysize_pix = 1000 };
        }

        Renderer {
            target,
            xsize,
            ysize,
            xsize_pix,
            ysize_pix,
            #[cfg(feature = "video")]
            last_frame: None,
            #[cfg(feature = "video")]
            position: None,
            #[cfg(feature = "video")]
            duration: None
        }
    }

    // TODO: Add pixel size
    #[cfg(feature = "video")]
    fn set_size(&mut self, xsize: usize, ysize: usize) -> MResult<()> {
        self.xsize = xsize;
        self.ysize = ysize;

        if let Some(ref frame) =  self.last_frame {
            let pos = self.position.unwrap_or(0);
            let dur = self.duration.unwrap_or(0);

            // Use send_image, because send_frame takes SampleRef
            self.send_image(frame)?;

            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();

            writeln!(stdout, "")?;
            writeln!(stdout, "{}", pos)?;
            writeln!(stdout, "{}", dur)?;
        }
        Ok(())
    }

    fn send_image(&self, image: &DynamicImage) -> MResult<()> {
        match self.target {
            RenderTarget::Sixel => self.print_sixel(image)?,
            RenderTarget::Unicode => self.print_unicode(image)?,
            RenderTarget::Kitty => self.print_kitty(image)?
        }

        Ok(())
    }

    #[cfg(feature = "video")]
    fn send_frame(&mut self,
                  frame: &gstreamer::sample::SampleRef,
                  position: u64,
                  duration: u64)
                  -> MResult<()> {
        let buffer = frame.get_buffer()
            .ok_or(format_err!("Couldn't get buffer from frame!"))?;
        let map = buffer.map_readable()
            .ok_or(format_err!("Couldn't get buffer from frame!"))?;

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        let img = image::load_from_memory_with_format(&map,
                                                      image::ImageFormat::PNM)?;
        let (_, height) = self.max_size(&img);

        match self.target {
            RenderTarget::Sixel => self.print_sixel(&img)?,
            RenderTarget::Unicode => self.print_unicode(&img)?,
            RenderTarget::Kitty => self.print_kitty(&img)?
        }

        self.last_frame = Some(img);
        self.position = Some(position as usize);
        self.duration = Some(duration as usize);

        // Empty line means end of frame
        writeln!(stdout, "")?;

        // Send size (in rows), position and duration
        writeln!(stdout, "{}", height)?;
        writeln!(stdout, "{}", position)?;
        writeln!(stdout, "{}", duration)?;

        Ok(())
    }

    pub fn render_image(&self, image: &DynamicImage) -> Vec<String> {
        use image::Pixel;
        let (xsize, ysize) = self.max_size(&image);

        // double height, because of half-height unicode
        let img = image.resize_exact(xsize as u32,
                                     ysize as u32 * 2,
                                     FilterType::Nearest).to_rgba();


        let rows = img.pixels()
            .collect::<Vec<_>>()
            .chunks(xsize as usize)
            .map(|line| line.to_vec())
            .collect::<Vec<Vec<_>>>();

        rows.chunks(2)
            .map(|rows| {
                rows[0]
                    .iter()
                    .zip(rows[1].iter())
                    .map(|(upper, lower)| {
                        let upper_color = upper.to_rgb();
                        let lower_color = lower.to_rgb();

                        format!("{}{}▀{}",
                                Fg(Rgb(upper_color[0], upper_color[1], upper_color[2])),
                                Bg(Rgb(lower_color[0], lower_color[1], lower_color[2])),
                                termion::style::Reset
                        )
                    }).collect()
            }).collect()
    }

    fn print_unicode(&self, img: &DynamicImage) -> MResult<()> {
        let rendered_img = self.render_image(img);
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        for line in rendered_img {
            writeln!(stdout, "{}", line)?;
        }

        Ok(())
    }

    fn print_kitty(&self, img: &DynamicImage) -> MResult<()> {
        let w = img.width();
        let h = img.height();

        let (max_x, max_y) = self.max_size(img);

        let img = img.to_rgb().into_vec();

        let mut file = std::fs::File::create("/tmp/img.raw").unwrap();
        file.write_all(&img)?;
        // Necessary?
        file.flush()?;
        std::mem::drop(file);

        let path = base64::encode("/tmp/img.raw");

        println!("\x1b_Gf=24,s={},v={},c={},r={},a=T,t=t;{}\x1b\\",
                 w,
                 h,
                 max_x,
                 max_y,
                 path);

        Ok(())
    }

    fn print_sixel(&self, img: &DynamicImage) -> MResult<()> {
        use sixel::optflags::*;

        // Currently faster than covnerting/resizing using image...
        img.save("/tmp/img.bmp")?;

        let encoder = Encoder::new().unwrap();

        let (xpix, ypix) = self.max_size_pix(img);

        encoder.set_quality(Quality::Low).ok();
        encoder.set_encode_policy(EncodePolicy::Fast).ok();
        encoder.set_color_option(ColorOption::Builtin("xterm256")).ok();
        encoder.set_width(SizeSpecification::Pixel(xpix as u64)).ok();
        encoder.set_height(SizeSpecification::Pixel(ypix as u64)).ok();
        encoder.encode_file(&std::path::PathBuf::from("/tmp/img.bmp")).ok();

        // End line printed by encoder
        println!("");

        Ok(())
    }

    pub fn max_size(&self, image: &DynamicImage) -> (usize, usize)
    {
        // TODO:  cell_ratio = xpix / ypix!
        let xsize = self.xsize;
        let ysize = self.ysize;
        let img_xsize = image.width() * 2; // Cells are roughly 2:1
        let img_ysize = image.height();
        let img_ratio = img_xsize as f32 / img_ysize as f32;

        let mut new_x;
        let mut new_y;

        if img_ratio < 1 as f32 {
            new_x = (ysize as f32 * img_ratio) as usize;
            new_y = ysize;
        } else {
            new_x = xsize;
            new_y = (xsize as f32 / img_ratio) as usize;
        }

        // ensure it fits within xsize
        if new_x > xsize {
            new_x = xsize;
            new_y = (xsize as f32 / img_ratio) as usize;
        }

        // ensure it fits within ysize
        if new_y > ysize {
            new_y = ysize;
            new_x = (ysize as f32 * img_ratio) as usize;
        }


        (new_x as usize, new_y as usize)
    }

    pub fn max_size_pix(&self, image: &DynamicImage) -> (usize, usize)
    {
        let xsize = self.xsize_pix;
        let ysize = self.ysize_pix;
        let img_xsize = image.width();
        let img_ysize = image.height();
        let img_ratio = img_xsize as f32 / img_ysize as f32;

        let mut new_x;
        let mut new_y;

        // tall / slim
        if img_ratio < 1 as f32 {
            new_x = (ysize as f32 * img_ratio) as usize;
            new_y = ysize;
        // short / wide
        } else {
            new_x = xsize;
            new_y = (xsize as f32 / img_ratio) as usize;
        }

        // ensure it fits within xsize
        if new_x > xsize {
            new_x = xsize;
            new_y = (xsize as f32 / img_ratio) as usize;
        }

        // ensure it fits within ysize
        if new_y > ysize {
            new_y = ysize;
            new_x = (ysize as f32 * img_ratio) as usize;
        }


        (new_x as usize, new_y as usize)
    }
}
