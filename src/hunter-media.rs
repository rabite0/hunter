// Based on https://github.com/jD91mZM2/termplay
// MIT License

use image::{RgbaImage, DynamicImage, GenericImageView};
use base64;

use termion::color::{Bg, Fg, Rgb};
#[cfg(feature = "video")]
use termion::input::TermRead;


#[cfg(feature = "video")]
use gstreamer::prelude::*;
#[cfg(feature = "video")]
use gstreamer_app;

use failure::{Error, format_err};

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
    let mut xpix = args.get(3)
        .expect("provide xsize in pixels")
        .parse::<usize>()
        .unwrap();
    let mut ypix = args.get(4)
        .expect("provide ysize in pixels")
        .parse::<usize>()
        .unwrap();
    let mut cell_ratio = args.get(5)
        .expect("Provide cell ratio")
        .parse::<f32>()
        .unwrap();
    let preview_type = args.get(6)
        .expect("Provide preview type")
        .parse::<String>()
        .unwrap();
    #[allow(unused_variables)]
    let autoplay = args.get(7)
        .expect("Autoplay?")
        .parse::<bool>()
        .unwrap();
    #[allow(unused_variables)]
    let mute = args.get(8)
        .expect("Muted?")
        .parse::<bool>()
        .unwrap();
    let target = args.get(9)
        .expect("Render target?")
        .parse::<String>()
        .unwrap();
    let path = args.get(10).expect("Provide path");

    let target = match target.as_str() {
        #[cfg(feature = "sixel")]
        "sixel" => RenderTarget::Sixel,
        "kitty" => RenderTarget::Kitty,
        "auto" => {
            let term = std::env::var("TERM").unwrap_or(String::from(""));
            match term.as_str() {
                "kitty" => RenderTarget::Kitty,
                #[cfg(feature = "sixel")]
                "xterm" => RenderTarget::Sixel,
                _ => RenderTarget::Unicode,
            }
        }
        _ => RenderTarget::Unicode
    };

    if target == RenderTarget::Unicode {
        xpix = xsize;
        ypix = ysize * 2;
        cell_ratio = 0.5;
    }



    let renderer = Renderer::new(target,
                                 xsize,
                                 ysize,
                                 xpix,
                                 ypix,
                                 cell_ratio);

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
    let max_size = renderer.max_size_pix(&img);

    let img = img.resize_exact(max_size.0 as u32,
                               max_size.1 as u32,
                               image::FilterType::Gaussian)
        .to_rgba();
    renderer.send_image(&img)?;

    Ok(())
}

trait ImgSize {
    fn size(&self) -> MResult<(usize, usize)>;
}

#[cfg(feature = "video")]
impl ImgSize for gstreamer::Sample {
    fn size(&self) -> MResult<(usize, usize)> {
        let size = || {
            let caps = self.as_ref().get_caps()?;
            let caps = caps.get_structure(0)?;
            let width = caps.get::<i32>("width")? as usize;
            let height = caps.get::<i32>("height")? as usize;
            Some((width, height))
        };
        size().ok_or(format_err!("Can't get size from sample!"))
    }
}

impl ImgSize for RgbaImage {
    fn size(&self) -> MResult<(usize, usize)> {
        let width = self.width() as usize;
        let height = self.height() as usize;
        Ok((width, height))
    }
}

impl ImgSize for DynamicImage {
    fn size(&self) -> MResult<(usize, usize)> {
        let width = self.width() as usize;
        let height = self.height() as usize;
        Ok((width, height))
    }
}


#[cfg(feature = "video")]
fn video_preview(path: &String,
                 renderer: Renderer,
                 autoplay: bool,
                 mute: bool)
                 -> MResult<()> {
    let gst = Gstreamer::new(path)?;

    let renderer = Arc::new(RwLock::new(renderer));
    let crenderer = renderer.clone();
    let cgst = gst.clone();
    gst.process_first_frame(&renderer)?;

    gst.appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::new()
            .new_sample({
                move |sink| {
                    let renderer = crenderer.clone();
                    let gst = cgst.clone();

                    let sample = match sink.pull_sample() {
                        Some(sample) => sample,
                        None => return Err(gstreamer::FlowError::Eos)
                    };

                    let pos = gst.position();
                    let dur = gst.duration();

                    std::thread::spawn(move || {
                        // This will lock make sure only one frame is being sent
                        // at a time
                        renderer.try_write()
                            .map(|mut r| r.new_frame(sample,
                                                     pos,
                                                     dur).unwrap())
                            .map_err(|_| {
                                // But if processing takes too long, reduce rate
                                let rate = gst.get_rate().unwrap();
                                gst.set_rate(rate-1)
                            }).ok();
                    });

                    Ok(gstreamer::FlowSuccess::Ok)
                }
            })
            .eos({
                move |_| {
                    std::process::exit(0);
                }
            })
            .build()
    );

    // Flush pipeline and restart with corrent resizing
    gst.stop()?;

    if autoplay {
        gst.start(mute)?;
    } else {
        gst.pause()?;
        gst.send_preroll(&renderer)?;
    }

    read_keys(gst.clone(), Some(renderer))?;

    Ok(())
}

#[cfg(feature = "video")]
fn read_keys(gst: Gstreamer,
             renderer: Option<Arc<RwLock<Renderer>>>) -> MResult<()> {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    loop {
        let input = stdin
            .read_line()?
            .unwrap_or_else(|| String::from("q"));


        match input.as_str() {
            "q" => return gst.stop(),
            ">" => {
                gst.seek_forward()?;
                renderer.as_ref().map(|r| {
                    if gst.get_state() == gstreamer::State::Paused {
                        gst.send_preroll(&r).unwrap();
                    }
                });
            },
            "<" => {
                gst.seek_backward()?;
                renderer.as_ref().map(|r| {
                    if gst.get_state() == gstreamer::State::Paused {
                        gst.send_preroll(&r).unwrap();
                    }
                });
            }
            "p" => gst.play()?,
            "a" => gst.pause()?,
            "m" => gst.mute()?,
            "u" => gst.unmute()?,
            "xy" => {
                if let Some(ref renderer) = renderer {
                    let xsize = stdin.read_line()?
                        .unwrap_or(String::from("0"))
                        .parse::<usize>()?;
                    let ysize = stdin.read_line()?
                        .unwrap_or(String::from("0"))
                        .parse::<usize>()?;
                    let mut xpix = stdin.read_line()?
                        .unwrap_or(String::from("0"))
                        .parse::<usize>()?;
                    let mut ypix = stdin.read_line()?
                        .unwrap_or(String::from("0"))
                        .parse::<usize>()?;
                    let cell_ratio = stdin.read_line()?
                        .unwrap_or(String::from("0"))
                        .parse::<f32>()?;
                    let mut renderer = renderer
                        .write()
                        .map_err(|_| format_err!("Renderer RwLock failed!"))?;

                    if renderer.target == RenderTarget::Unicode {
                        xpix = xsize;
                        ypix = ysize*2;
                    }


                    renderer.set_widget_size(xsize, ysize, xpix, ypix, cell_ratio)?;
                    match renderer.last_frame {
                        Some(ref sample) => {
                            let (max_x, max_y) = renderer.max_size_pix(sample);
                            gst.set_scaling(max_x, max_y)?;
                        }
                        _ => {}
                    }


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
    let gst = Gstreamer::new(path)?;
    let tgst = gst.clone();

    // Since events don't work with audio files...
    std::thread::spawn(move || -> MResult<()> {
        let mut last_pos = None;
        let sleep_duration = std::time::Duration::from_millis(50);
        let mut stdout = std::io::stdout();
        loop {
            std::thread::sleep(sleep_duration);
            let gst = tgst.clone();

            let position = gst.position();
            let duration = gst.duration();

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

    if autoplay && !mute {
        gst.start(mute)?;
    }

    read_keys(gst, None)?;

    Ok(())
}

#[cfg(feature = "video")]
#[derive(Clone)]
struct Gstreamer {
    player: gstreamer::Element,
    appsink: gstreamer_app::AppSink,
    videorate: gstreamer::Element,
}

#[cfg(feature = "video")]
impl Gstreamer {
    fn new(file: &str) -> MResult<Gstreamer> {
        use gstreamer::{Element, ElementFactory, GhostPad, Bin};
        gstreamer::init()?;

        let player = ElementFactory::make("playbin", None)
            .ok_or(format_err!("Can't create playbin"))?;

        let videorate = ElementFactory::make("videorate", None)
            .ok_or(format_err!("Can't create videorate element"))?;

        let sink = ElementFactory::make("appsink", None)
            .ok_or(format_err!("Can't create appsink"))?;

        let appsink = sink.clone()
            .downcast::<gstreamer_app::AppSink>()
            .unwrap();

        let elems = &[&videorate,  //&videoscale,
                      &sink];

        let bin = Bin::new(None);

        bin.add_many(elems)?;
        Element::link_many(elems)?;

        // make input for bin point to first element
        let sink = elems[0].get_static_pad("sink").unwrap();
        let ghost = GhostPad::new(Some("sink"), &sink)
            .ok_or(format_err!("Can't create GhostPad"))?;

        ghost.set_active(true)?;
        bin.add_pad(&ghost)?;

        appsink.set_drop(true);
        appsink.set_max_buffers(4);

        videorate.set_property("drop-only", &true)?;
        //videorate.set_property("max-rate", &1)?;

        let uri = format!("file://{}", &file);

        player.set_property("video-sink", &bin.upcast::<gstreamer::Element>())?;
        player.set_property("uri", &uri)?;

        use gstreamer::prelude::*;

        Ok(Gstreamer {
            player,
            appsink,
            videorate,
        })
    }

    pub fn change_format(&self, format: gstreamer::Caps) -> MResult<()> {
        use gstreamer::Element;
        use gstreamer_video::prelude::*;

        let state = self.get_state();
        self.pause()?;


        let appsink = self.appsink.clone()
            .upcast::<Element>();

        Element::unlink_many(&[&self.videorate, &appsink]);

        self.appsink.set_caps(Some(&format));

        Element::link_many(&[&self.videorate, &appsink])?;

        std::thread::sleep(std::time::Duration::from_millis(100));
        self.player.set_state(state)?;



        Ok(())
    }

    pub fn process_first_frame(&self,
                               renderer: &Arc<RwLock<Renderer>>) -> MResult<()> {
        self.pause()?;

        let sample = self.appsink.pull_preroll()
            .ok_or_else(|| format_err!("Couldn't read first frame!"))?;

        let (max_x, max_y) = renderer.read()
            .map_err(|_| format_err!("Failed at locking renderer!"))?
            .max_size_pix(&sample);

        self.set_scaling(max_x, max_y)?;

        Ok(())
    }


    pub fn send_preroll(&self,
                       renderer: &Arc<RwLock<Renderer>>) -> MResult<()> {
        let appsink = self.appsink.downcast_ref::<gstreamer_app::AppSink>().unwrap();
        let sample = appsink.pull_preroll().unwrap();
        let pos = self.position();
        let dur = self.duration();
        renderer.write().unwrap().new_frame(sample, pos, dur)
    }

    pub fn set_scaling(&self, x: usize, y: usize) -> MResult<()> {
        use gstreamer::Caps;

        let caps =
            format!("video/x-raw,format=RGBA,width={},height={}",
                    x,
                    y);
        let caps = Caps::from_string(&caps).unwrap();

        self.change_format(caps)
    }

    pub fn get_rate(&self) -> MResult<i32> {
        let rate = self.videorate
            .get_property("max-rate")?
            .downcast::<i32>().unwrap()
            .get()
            .ok_or_else(|| format_err!("No video rate???"))?;

        if rate == 2147483647 {
            // Sane defalt fps cap if rendering is too slow
            Ok(30)
        } else {
            Ok(rate)
        }
    }

    pub fn set_rate(&self, rate: i32) -> MResult<()> {
        self.videorate.set_property("max-rate", &rate)?;
        Ok(())
    }

    pub fn position(&self) -> usize {
        self.player.query_position::<gstreamer::ClockTime>()
            .map(|p| p.seconds().unwrap_or(0))
            .unwrap_or(0) as usize
    }

    pub fn duration(&self) -> usize {
        self.player.query_duration::<gstreamer::ClockTime>()
            .map(|d| d.seconds().unwrap_or(0))
            .unwrap_or(0) as usize
    }

    pub fn set_state(&self, state: gstreamer::State) -> MResult<()> {
        self.player.set_state(state)?;
        // HACK: How to sync properly?
        std::thread::sleep(std::time::Duration::from_millis(100));

        Ok(())
    }

    pub fn pause(&self) -> MResult<()> {
        self.set_state(gstreamer::State::Paused)?;

        Ok(())
    }

    pub fn mute(&self) -> MResult<()> {
        Ok(self.player.set_property("volume", &0.0)?)
    }

    pub fn unmute(&self) -> MResult<()> {
        Ok(self.player.set_property("volume", &1.0)?)
    }

    pub fn get_state(&self) -> gstreamer::State {
        let timeout = gstreamer::ClockTime::from_seconds(1);
        let state = self.player.get_state(timeout);

        state.1
    }

    pub fn start(&self, mute: bool) -> MResult<()> {
        if mute {
            self.mute()?;
        }
        self.play()
    }

    pub fn play(&self) -> MResult<()> {
        self.set_state(gstreamer::State::Playing)
    }

    pub fn stop(&self) -> MResult<()> {
        self.set_state(gstreamer::State::Ready)
    }

    pub fn seek_forward(&self) -> MResult<()> {
        let seek_time = gstreamer::ClockTime::from_seconds(5);
        if let Some(mut time) = self.player
            .query_position::<gstreamer::ClockTime>() {
                time += seek_time;

                self.player.seek_simple(
                    gstreamer::SeekFlags::FLUSH,
                    gstreamer::format::GenericFormattedValue::Time(time)
                )?;
            }
        Ok(())
    }

    pub fn seek_backward(&self) -> MResult<()> {
        let seek_time = gstreamer::ClockTime::from_seconds(5);
        if let Some(mut time) = self.player
            .query_position::<gstreamer::ClockTime>() {
                if time >= seek_time {
                    time -= seek_time;
                } else {
                    time = gstreamer::ClockTime(Some(0));
                }

                self.player.seek_simple(
                    gstreamer::SeekFlags::FLUSH,
                    gstreamer::format::GenericFormattedValue::Time(time)
                )?;
            }
        Ok(())
    }
}


trait WithRaw {
    fn with_raw(&self,
                fun: impl FnOnce(&[u8]) -> MResult<()>)
                -> MResult<()>;
}

#[cfg(feature = "video")]
impl WithRaw for gstreamer::Sample {
    fn with_raw(&self,
                fun: impl FnOnce(&[u8]) -> MResult<()>)
                -> MResult<()> {
        let buffer = self.get_buffer()
            .ok_or(format_err!("Couldn't get buffer from frame!"))?;

        let map = buffer.map_readable()
            .ok_or(format_err!("Couldn't get buffer from frame!"))?;

        fun(map.as_slice())
    }
}

// Mostly for plain old images, since they come from image::open
impl WithRaw for RgbaImage {
    fn with_raw(&self,
                fun: impl FnOnce(&[u8]) -> MResult<()>)
                -> MResult<()> {
        let bytes = self.as_flat_samples();

        fun(bytes.as_slice())
    }
}

#[derive(PartialEq)]
enum RenderTarget {
    Unicode,
    #[cfg(feature = "sixel")]
    Sixel,
    Kitty
}

impl RenderTarget {
    fn send_image(&self,
                  img: &(impl WithRaw+ImgSize),
                  context: &Renderer) -> MResult<()> {
        match self {
            #[cfg(feature = "sixel")]
            RenderTarget::Sixel => self.print_sixel(img)?,
            RenderTarget::Unicode => self.print_unicode(img)?,
            RenderTarget::Kitty => self.print_kitty(img, context)?
        }
        Ok(())
    }

    fn print_unicode(&self, img: &(impl WithRaw+ImgSize)) -> MResult<()> {
        let (xsize, _) = img.size()?;

        img.with_raw(move |raw| -> MResult<()> {
            let lines = raw.chunks(4*xsize*2).map(|two_lines_colors| {
                let (upper_line,lower_line) = two_lines_colors.split_at(4*xsize);
                upper_line.chunks(4)
                    .zip(lower_line.chunks(4))
                    .map(|(upper, lower)| {
                        format!("{}{}▀{}",
                                Fg(Rgb(upper[0], upper[1], upper[2])),
                                Bg(Rgb(lower[0], lower[1], lower[2])),
                                termion::style::Reset
                        )
                    }).collect::<String>()
            }).collect::<Vec<String>>();

            for line in lines {
                println!("{}", line);
            }

            println!("");

            Ok(())
        })
    }

    fn print_kitty(&self,
                   img: &(impl WithRaw+ImgSize),
                   context: &Renderer) -> MResult<()> {
        let (w,h) = context.max_size(img);
        let (img_x, img_y) = img.size()?;

        img.with_raw(move |raw| -> MResult<()> {
            let mut file = std::fs::File::create("/tmp/img.raw.new")?;
            file.write_all(raw)?;
            file.flush()?;
            std::fs::rename("/tmp/img.raw.new", "/tmp/img.raw")?;

            let path = base64::encode("/tmp/img.raw");

            print!("\x1b_Ga=d\x1b\\");
            println!("\x1b_Gf=32,s={},v={},c={},r={},a=T,t=f;{}\x1b\\",
                     img_x,
                     img_y,
                     w,
                     h,
                     path);
            println!("");

            Ok(())
        })
    }

    #[cfg(feature = "sixel")]
    fn print_sixel(&self, img: &(impl WithRaw+ImgSize)) -> MResult<()> {
        use sixel_rs::encoder::{Encoder, QuickFrameBuilder};
        use sixel_rs::optflags::EncodePolicy;

        let (xpix, ypix) = img.size()?;

        img.with_raw(move |raw| -> MResult<()> {
            let sixfail = |e| format_err!("Sixel failed with: {:?}", e);
            let encoder = Encoder::new()
                .map_err(sixfail)?;

            encoder.set_encode_policy(EncodePolicy::Fast)
                .map_err(sixfail)?;

            let frame = QuickFrameBuilder::new()
                .width(xpix)
                .height(ypix)
                .format(sixel_sys::PixelFormat::RGBA8888)
                .pixels(raw.to_vec());

            encoder.encode_bytes(frame)
                .map_err(sixfail)?;

            // No end of line printed by encoder
            println!("");
            println!("");

            Ok(())
        })
    }
}

struct Renderer {
    target: RenderTarget,
    xsize: usize,
    ysize: usize,
    xpix: usize,
    ypix: usize,
    cell_ratio: f32,
    #[cfg(feature = "video")]
    last_frame: Option<gstreamer::Sample>,
    #[cfg(feature = "video")]
    position: usize,
    #[cfg(feature = "video")]
    duration: usize,
}

impl Renderer {
    fn new(target: RenderTarget,
           xsize: usize,
           ysize: usize,
           mut xpix: usize,
           mut ypix: usize,
           cell_ratio: f32) -> Renderer {

        #[cfg(feature = "sixel")]
        match std::env::var("TERM") {
            Ok(term) => {
                if term == "xterm" &&
                    target == RenderTarget::Sixel {
                // xterm has a hard limit on graphics size
                // maybe splitting the image into parts would work?
                    if xpix > 1000 { xpix = 1000 };
                    if ypix > 1000 { ypix = 1000 };
                }
            }
            _ => {}
        }

        Renderer {
            target,
            xsize,
            ysize,
            xpix,
            ypix,
            cell_ratio,
            #[cfg(feature = "video")]
            last_frame: None,
            #[cfg(feature = "video")]
            position: 0,
            #[cfg(feature = "video")]
            duration: 0,
        }
    }

    #[cfg(feature = "video")]
    fn set_widget_size(&mut self,
                      xsize: usize,
                      ysize: usize,
                      xpix: usize,
                      ypix: usize,
                      cell_ratio: f32) -> MResult<()> {
        self.xsize = xsize;
        self.ysize = ysize;
        self.xpix = xpix;
        self.ypix = ypix;
        self.cell_ratio = cell_ratio;

        self.resend_scaled_frame()?;
        Ok(())
    }

    #[cfg(feature = "video")]
    fn send_media_meta(&self, frame: &impl ImgSize) -> MResult<()> {
        let (_, height) = self.max_size(frame);

        println!("{}", height+1);
        println!("{}", self.position);
        println!("{}", self.duration);

        Ok(())
    }




    fn send_image(&self, image: &(impl WithRaw+ImgSize)) -> MResult<()> {
        self.target.send_image(image, &self)?;

        Ok(())
    }

    #[cfg(feature = "video")]
    fn new_frame(&mut self,
                 frame: gstreamer::sample::Sample,
                 position: usize,
                 duration: usize)
                 -> MResult<()> {
        self.position = position;
        self.duration = duration;

        self.target.send_image(&frame, &self)?;
        self.send_media_meta(&frame)?;

        self.last_frame = Some(frame);

        Ok(())
    }

    #[cfg(feature = "video")]
    fn resend_scaled_frame(&self) -> MResult<()> {
        use image::{ImageBuffer, Rgba};
        self.last_frame.as_ref().map(|frame| {
            let (xpix, ypix) = frame.size()?;
            frame.with_raw(|raw| {
                let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(xpix as u32,
                                                                     ypix as u32,
                                                                     raw.to_vec())
                    .ok_or(format_err!("Couldn't load last frame for rescaling!"))?;

                let img = DynamicImage::ImageRgba8(img);
                let (max_x, max_y) = self.max_size_pix(&img);
                let img = img.resize_exact(max_x as u32,
                                           max_y as u32,
                                           image::FilterType::Gaussian).to_rgba();

                self.send_image(&img)?;
                self.send_media_meta(&img)?;
                Ok(())
            })
        });
        Ok(())
    }

    pub fn max_size(&self, image: &impl ImgSize) -> (usize, usize)
    {
        let xsize = self.xsize;
        let ysize = self.ysize;
        let (img_xsize, img_ysize) = image.size().unwrap();
        // Cells are not square, but almost 2:1
        let img_ratio = (img_xsize as f32 / img_ysize as f32) / self.cell_ratio;


        let (new_x, new_y) = fill_ratio(img_ratio, xsize, ysize);


        (new_x as usize, new_y as usize)
    }

    pub fn max_size_pix(&self, image: &impl ImgSize) -> (usize, usize)
    {
        let xsize = self.xpix;
        let ysize = self.ypix;
        let (img_xsize, img_ysize) = image.size().unwrap();
        let img_ratio = img_xsize as f32 / img_ysize as f32;

        let (new_x, mut new_y) = fill_ratio(img_ratio, xsize, ysize);

        if self.target == RenderTarget::Unicode {
            let rem = new_y % 2;
            new_y -= rem;
        }

        (new_x as usize, new_y as usize)
    }
}

fn fill_ratio(ratio: f32, max_x: usize, max_y: usize) -> (usize, usize) {
    let mut new_x;
    let mut new_y;

    // tall / slim
    if ratio < 1 as f32 {
        new_x = (max_y as f32 * ratio) as usize;
        new_y = max_y;
        // short / wide
    } else {
        new_x = max_x;
        new_y = (max_x as f32 / ratio) as usize;
    }

    // ensure it fits within max_x
    if new_x > max_x {
        new_x = max_x;
        new_y = (max_x as f32 / ratio) as usize;
    }

    // ensure it fits within max_y
    if new_y > max_y {
        new_y = max_y;
        new_x = (max_y as f32 * ratio) as usize;
    }

    (new_x, new_y)
}
