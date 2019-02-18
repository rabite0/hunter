use termion::event::{Event, Key};
use unicode_width::UnicodeWidthStr;

use std::path::{Path, PathBuf};
use std::io::Write;

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files};
use crate::term;
use crate::widget::{Widget};

#[derive(PartialEq)]
pub struct ListView<T>
where
    T: Send,
{
    pub content: T,
    lines: usize,
    selection: usize,
    offset: usize,
    buffer: Vec<String>,
    coordinates: Coordinates,
    seeking: bool,
}

impl<T> ListView<T>
where
    ListView<T>: Widget,
    T: Send
{
    pub fn new(content: T) -> ListView<T> {
        let view = ListView::<T> {
            content: content,
            lines: 0,
            selection: 0,
            offset: 0,
            buffer: Vec::new(),
            coordinates: Coordinates {
                size: Size((1, 1)),
                position: Position((1, 1)),
            },
            seeking: false
        };
        view
    }

    fn move_up(&mut self) {
        if self.selection == 0 {
            return;
        }

        if self.selection - self.offset <= 0 {
            self.offset -= 1;
        }

        self.selection -= 1;
        self.seeking = false;
    }
    fn move_down(&mut self) {
        let lines = self.lines;
        let y_size = self.coordinates.ysize() as usize;

        if self.selection == lines - 1 {
            return;
        }

        if self.selection + 1 >= y_size && self.selection + 1 - self.offset >= y_size {
            self.offset += 1;
        }

        self.selection += 1;
        self.seeking = false;
    }

    pub fn get_selection(&self) -> usize {
        self.selection
    }

    fn set_selection(&mut self, position: usize) {
        let ysize = self.coordinates.ysize() as usize;
        let mut offset = 0;

        while position + 2
            >= ysize + offset {
            offset += 1
        }

        self.offset = offset;
        self.selection = position;
    }

    fn render_line(&self, file: &File) -> String {
        let name = &file.name;
        let (size, unit) = file.calculate_size().unwrap();

        let selection_gap = "  ".to_string();
        let (name, selection_color) =  if file.is_selected() {
            (selection_gap + name, crate::term::color_yellow())
        } else { (name.clone(), "".to_string()) };


        let xsize = self.get_coordinates().xsize();
        let sized_string = term::sized_string(&name, xsize);
        let size_pos = xsize - (size.to_string().len() as u16
                                + unit.to_string().len() as u16);
        let padding = sized_string.len() - sized_string.width_cjk();
        let padding = xsize - padding as u16;

        format!(
            "{}{}{}{}{}{}{}",
            termion::cursor::Save,
            match &file.color {
                Some(color) => format!("{}{}{:padding$}{}",
                                       term::from_lscolor(color),
                                       selection_color,
                                       &sized_string,
                                       term::normal_color(),
                                       padding = padding as usize),
                _ => format!("{}{}{:padding$}{}",
                             term::normal_color(),
                             selection_color,
                             &sized_string,
                             term::normal_color(),
                             padding = padding as usize),
            } ,
            termion::cursor::Restore,
            termion::cursor::Right(size_pos),
            term::highlight_color(),
            size,
            unit
        )
    }
}

impl ListView<Files>
{
    pub fn selected_file(&self) -> &File {
        let selection = self.selection;
        let file = &self.content[selection];
        file
    }

    pub fn selected_file_mut(&mut self) -> &mut File {
        let selection = self.selection;
        let file = &mut self.content.files[selection];
        file
    }

    pub fn clone_selected_file(&self) -> File {
        let selection = self.selection;
        let file = self.content[selection].clone();
        file
    }

    pub fn grand_parent(&self) -> Option<PathBuf> {
        self.selected_file().grand_parent()
    }

    pub fn goto_grand_parent(&mut self) {
        match self.grand_parent() {
            Some(grand_parent) => self.goto_path(&grand_parent),
            None => self.show_status("Can't go further!"),
        }
    }

    fn goto_selected(&mut self) {
        let path = self.selected_file().path();

        self.goto_path(&path);
    }

    pub fn goto_path(&mut self, path: &Path) {
        match crate::files::Files::new_from_path(path) {
            Ok(files) => {
                self.content = files;
                self.selection = 0;
                self.offset = 0;
                self.refresh();
            }
            Err(err) => {
                self.show_status(&format!("Can't open this path: {}", err));
                return;
            }
        }
    }

    pub fn select_file(&mut self, file: &File) {
        let pos = self
            .content
            .files
            .iter()
            .position(|item| item == file)
            .unwrap_or(0);
        self.set_selection(pos);
    }

    fn cycle_sort(&mut self) {
        let file = self.clone_selected_file();
        self.content.cycle_sort();
        self.content.sort();
        self.select_file(&file);
        self.refresh();
        self.show_status(&format!("Sorting by: {}", self.content.sort));
    }

    fn reverse_sort(&mut self) {
        let file = self.clone_selected_file();
        self.content.reverse_sort();
        self.content.sort();
        self.select_file(&file);
        self.refresh();
        self.show_status(&format!("Reversed sorting by: {}", self.content.sort));
    }

    fn select_next_mtime(&mut self) {
        let file = self.clone_selected_file();
        let dir_settings = self.content.dirs_first;
        let sort_settings = self.content.sort;

        self.content.dirs_first = false;
        self.content.sort = crate::files::SortBy::MTime;
        self.content.sort();

        self.select_file(&file);

        if self.seeking == false || self.selection + 1 == self.content.len() {
            self.selection = 0;
            self.offset = 0;
        } else {
            self.move_down();
         }

        let file = self.clone_selected_file();
        self.content.dirs_first = dir_settings;
        self.content.sort = sort_settings;
        self.content.sort();
        self.select_file(&file);
        self.seeking = true;

        self.refresh();
    }

    fn select_prev_mtime(&mut self) {
        let file = self.clone_selected_file();
        let dir_settings = self.content.dirs_first;
        let sort_settings = self.content.sort;

        self.content.dirs_first = false;
        self.content.sort = crate::files::SortBy::MTime;
        self.content.sort();

        self.select_file(&file);

        if self.seeking == false || self.selection == 0 {
            self.set_selection(self.content.len() - 1);
        } else {
            self.move_up();
        }

        let file = self.clone_selected_file();
        self.content.dirs_first = dir_settings;
        self.content.sort = sort_settings;
        self.content.sort();
        self.select_file(&file);
        self.seeking = true;

        self.refresh();
    }

    fn toggle_hidden(&mut self) {
        let file = self.clone_selected_file();
        self.content.toggle_hidden();
        self.content.reload_files();
        self.select_file(&file);
        self.refresh();
    }

    fn toggle_dirs_first(&mut self) {
        let file = self.clone_selected_file();
        self.content.dirs_first = !self.content.dirs_first;
        self.content.sort();
        self.select_file(&file);
        self.refresh();
        self.show_status(&format!("Direcories first: {}", self.content.dirs_first));
    }

    fn multi_select_file(&mut self) {
        let file = self.selected_file_mut();
        file.toggle_selection();
        self.move_down();
        self.refresh();
    }

    fn exec_cmd(&mut self) {
        let selected_files = self.content.get_selected();
        let file_names
            = selected_files.iter().map(|f| f.name.clone()).collect::<Vec<String>>();

        match self.minibuffer("exec ($s for selected file(s))") {
            Some(cmd) => {
                self.show_status(&format!("Running: \"{}\"", &cmd));

                let filename = self.selected_file().name.clone();

                let cmd = if file_names.len() == 0 {
                    cmd.replace("$s", &format!("{}", &filename))
                } else {
                    let args = file_names.iter().map(|f| {
                        format!(" \"{}\" ", f)
                    }).collect::<String>();
                    let clean_cmd = cmd.replace("$s", "");

                    clean_cmd + &args
                };

                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .status();
                let mut bufout = std::io::BufWriter::new(std::io::stdout());
                write!(bufout, "{}{}",
                       termion::style::Reset,
                       termion::clear::All).unwrap();

                match status {
                    Ok(status) => self.show_status(&format!("\"{}\" exited with {}",
                                                            cmd, status)),
                    Err(err) => self.show_status(&format!("Can't run this \"{}\": {}",
                                                          cmd, err)),
                }
            }
            None => self.show_status(""),
        }
    }

    fn render(&self) -> Vec<String> {
        let ysize = self.get_coordinates().ysize() as usize;
        let offset = self.offset;
        self.content
            .files
            .iter()
            .skip(offset)
            .take(ysize)
            .map(|file| self.render_line(&file))
            .collect()
    }
}

impl Widget for ListView<Files> {
    fn get_coordinates(&self) -> &Coordinates {
        &self.coordinates
    }
    fn set_coordinates(&mut self, coordinates: &Coordinates) {
        if self.coordinates == *coordinates {
            return;
        }
        self.coordinates = coordinates.clone();
        self.refresh();
    }
    fn refresh(&mut self) {
        let visible_file_num = self.selection + self.get_coordinates().ysize() as usize;
        self.content.meta_upto(visible_file_num);
        self.lines = self.content.len();
        self.buffer = self.render();
    }


    fn get_drawlist(&self) -> String {
        let mut output = term::reset();
        let ysize = self.get_coordinates().ysize();
        let (xpos, ypos) = self.coordinates.position().position();

        output += &self
            .buffer
            .iter()
            //.skip(self.offset)
            //.take(ysize as usize)
            .enumerate()
            .map(|(i, item)| {
                let mut output = term::normal_color();

                if i == (self.selection - self.offset) {
                    output += &term::invert();
                }
                output += &format!(
                    "{}{}{}",
                    term::goto_xy(xpos, i as u16 + ypos),
                    item,
                    term::reset()
                );
                String::from(output)
            })
            .collect::<String>();

        output += &self.get_redraw_empty_list(self.buffer.len());

        output
    }
    fn render_header(&self) -> String {
        format!("{} files", self.content.len())
    }

    fn on_key(&mut self, key: Key) {
        match key {
            Key::Up | Key::Char('p') => {
                self.move_up();
                self.refresh();
            }
            Key::Char('P') => { for _ in 0..10 { self.move_up() } self.refresh(); }
            Key::Char('N') => { for _ in 0..10 { self.move_down() } self.refresh(); }
            Key::Down | Key::Char('n') => {
                self.move_down();
                self.refresh();
            }
            Key::Left => self.goto_grand_parent(),
            Key::Right => self.goto_selected(),
            Key::Char(' ') => self.multi_select_file(),
            Key::Char('h') => self.toggle_hidden(),
            Key::Char('r') => self.reverse_sort(),
            Key::Char('s') => self.cycle_sort(),
            Key::Char('K') => self.select_next_mtime(),
            Key::Char('k') => self.select_prev_mtime(),
            Key::Char('d') => self.toggle_dirs_first(),
            Key::Char('!') => self.exec_cmd(),
            _ => {
                self.bad(Event::Key(key));
            }
        }
    }
}
