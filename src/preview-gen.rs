// Based on https://github.com/jD91mZM2/termplay
// MIT License

use image::{Pixel, FilterType, DynamicImage, GenericImageView};

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

use rayon::prelude::*;

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
    #[cfg(feature = "video")]
    let preview_type = args.get(3)
        .expect("Provide preview type")
        .parse::<String>()
        .unwrap();
    #[cfg(feature = "video")]
    let autoplay = args.get(4)
        .expect("Autoplay?")
        .parse::<bool>()
        .unwrap();
    #[cfg(feature = "video")]
    let mute = args.get(5)
        .expect("Muted?")
        .parse::<bool>()
        .unwrap();
    let path = args.get(6).expect("Provide path");

    #[cfg(feature = "video")]
    let result =
        match preview_type.as_ref() {
            "video" => video_preview(path, xsize, ysize, autoplay, mute),
            "image" => image_preview(path, xsize, ysize),
            "audio" => audio_preview(path, autoplay, mute),
            _ => { panic!("Available types: video/image/audio") }
        };



    #[cfg(not(feature = "video"))]
    let result = image_preview(path, xsize, ysize);

    if result.is_err() {
        println!("{:?}", &result);
        result
    } else {
        Ok(())
    }
}

fn image_preview(path: &str,
                 xsize: usize,
                 ysize: usize) -> MResult<()> {
    let img = image::open(&path)?;

    let renderer = Renderer::new(xsize, ysize);

    renderer.send_image(&img)?;
    Ok(())
}

#[cfg(feature = "video")]
fn video_preview(path: &String,
                 xsize: usize,
                 ysize: usize,
                 autoplay: bool,
                 mute: bool)
                 -> MResult<()> {

    let (player, appsink) = make_gstreamer()?;

    let uri = format!("file://{}", &path);

    player.set_property("uri", &uri)?;


    let renderer = Renderer::new(xsize, ysize);
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

                    if let Ok(mut renderer) = crenderer.write() {
                        match renderer.send_frame(&*sample,
                                                  position,
                                                  duration) {
                            Ok(()) => {
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
                            Err(err) => {
                                println!("{:?}", err);
                                gstreamer::FlowReturn::Error
                            }
                        }
                    } else { gstreamer::FlowReturn::Error }

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
    let (player, _) = make_gstreamer()?;

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
            // Send position and duration
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
pub fn make_gstreamer() -> MResult<(gstreamer::Element,
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


    videorate.set_property("max-rate", &60)?;

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


struct Renderer {
    xsize: usize,
    ysize: usize,
    #[cfg(feature = "video")]
    last_frame: Option<DynamicImage>,
    #[cfg(feature = "video")]
    position: Option<usize>,
    #[cfg(feature = "video")]
    duration: Option<usize>
}

impl Renderer {
    fn new(xsize: usize, ysize: usize) -> Renderer {
        Renderer {
            xsize,
            ysize,
            #[cfg(feature = "video")]
            last_frame: None,
            #[cfg(feature = "video")]
            position: None,
            #[cfg(feature = "video")]
            duration: None
        }
    }


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
        let rendered_img = self.render_image(image);
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        for line in rendered_img {
            writeln!(stdout, "{}", line)?;
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

        let img = image::load_from_memory_with_format(&map,
                                                      image::ImageFormat::PNM)?;

        let rendered_img = self.render_image(&img);

        self.last_frame = Some(img);
        self.position = Some(position as usize);
        self.duration = Some(duration as usize);

        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();

        for line in rendered_img {
            writeln!(stdout, "{}", line)?;
        }

        // Empty line means end of frame
        writeln!(stdout, "")?;

        // Send position and duration
        writeln!(stdout, "{}", position)?;
        writeln!(stdout, "{}", duration)?;

        Ok(())
    }

    pub fn render_image(&self, image: &DynamicImage) -> Vec<String> {
        let (xsize, ysize) = self.max_size(&image);

        let img = image.resize_exact(xsize as u32,
                                     ysize as u32,
                                     FilterType::Nearest).to_rgba();


        let rows = img.pixels()
            .collect::<Vec<_>>()
            .chunks(xsize as usize)
            .map(|line| line.to_vec())
            .collect::<Vec<Vec<_>>>();

        rows.par_chunks(2)
            .map(|rows| {
                rows[0]
                    .par_iter()
                    .zip(rows[1].par_iter())
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

    pub fn max_size(&self, image: &DynamicImage) -> (usize, usize)
    {
        let xsize = self.xsize;
        let ysize = self.ysize;
        let img_xsize = image.width();
        let img_ysize = image.height();
        let img_ratio = img_xsize as f32 / img_ysize as f32;

        let mut new_x = xsize;
        let mut new_y;

        new_y = if img_ratio < 1 as f32 {
            (xsize as f32 * img_ratio) as usize
        } else {
            (xsize as f32 / img_ratio) as usize
        };

        // Multiply by two because of half-block
        if new_y > ysize*2 {
            new_y = self.ysize * 2;

            new_x = if img_ratio < 1 as f32 {
                (ysize as f32 / img_ratio) as usize * 2
            } else {
                (ysize as f32 * img_ratio) as usize * 2
            };
        }

        // To make half-block encoding easier, y should be divisible by 2
        if new_y as u32 % 2 == 1 {
            new_y += 1;
        }


        (new_x as usize, new_y as usize)
    }
}
