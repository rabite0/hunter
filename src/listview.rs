use rayon::prelude::*;
use termion::event::{Event, Key};
use unicode_width::UnicodeWidthStr;

use std::path::{Path, PathBuf};

use crate::coordinates::{Coordinates, Position, Size};
use crate::files::{File, Files};
use crate::term;
use crate::widget::Widget;

// Maybe also buffer drawlist for efficiency when it doesn't change every draw

#[derive(PartialEq)]
pub struct ListView<T>
where
    T: Send,
{
    pub content: T,
    selection: usize,
    offset: usize,
    buffer: Vec<String>,
    // dimensions: (u16, u16),
    // position: (u16, u16),
    coordinates: Coordinates,
    seeking: bool,
}

impl<T> ListView<T>
where
    ListView<T>: Widget,
    T: Send,
{
    pub fn new(content: T) -> Self {
        let view = ListView::<T> {
            content: content,
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
        let lines = self.buffer.len();
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

        while position >= ysize - 2 + offset {
            offset += 1
        }

        self.offset = offset;
        self.selection = position;
    }

    fn render_line(&self, file: &File) -> String {
        let name = &file.name;
        let (size, unit) = file.calculate_size();

        let xsize = self.get_size().xsize();
        let sized_string = term::sized_string(&name, xsize);
        let size_pos = xsize - (size.to_string().len() as u16
                                + unit.to_string().len() as u16);
        let padding = sized_string.len() - sized_string.width_cjk();
        let padding = xsize - padding as u16;

        format!(
            "{}{}{}{}{}{}{}",
            termion::cursor::Save,
            match &file.color {
                Some(color) => format!("{}{:padding$}",
                                       term::from_lscolor(color),
                                       &sized_string,
                                       padding = padding as usize),
                _ => format!("{}{:padding$}",
                             term::normal_color(),
                             &sized_string,
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
where
    ListView<Files>: Widget,
    Files: std::ops::Index<usize, Output = File>,
    Files: std::marker::Sized,
{
    pub fn selected_file(&self) -> &File {
        let selection = self.selection;
        let file = &self.content[selection];
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
            .par_iter()
            .position_any(|item| item == file)
            .unwrap();
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

    fn exec_cmd(&mut self) {
        match self.minibuffer("exec ($s for selected files)") {
            Some(cmd) => {
                self.show_status(&format!("Running: \"{}\"", &cmd));

                let filename = self.selected_file().name.clone();
                let cmd = cmd.replace("$s", &format!("{}", &filename));

                let status = std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&cmd)
                    .status();
                match status {
                    Ok(status) => self.show_status(&format!("\"{}\" exited with {}", cmd, status)),
                    Err(err) => self.show_status(&format!("Can't run this \"{}\": {}", cmd, err)),
                }
            }
            None => self.show_status(""),
        }
    }

    fn render(&self) -> Vec<String> {
        self.content
            .files
            .par_iter()
            .map(|file| self.render_line(&file))
            .collect()
    }
}

impl Widget for ListView<Files> {
    fn get_size(&self) -> &Size {
        &self.coordinates.size
    }
    fn get_position(&self) -> &Position {
        &self.coordinates.position
    }
    fn set_size(&mut self, size: Size) {
        self.coordinates.size = size;
    }
    fn set_position(&mut self, position: Position) {
        self.coordinates.position = position;
    }
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
        self.buffer = self.render();
    }


    fn get_drawlist(&self) -> String {
        let mut output = term::reset();
        let (_, ysize) = self.get_size().size();
        let (xpos, ypos) = self.coordinates.position().position();

        output += &self
            .buffer
            .par_iter()
            .skip(self.offset)
            .take(ysize as usize)
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
            Key::Char('P') => for _ in 0..10 { self.move_up() }
            Key::Char('N') => for _ in 0..10 { self.move_down() }
            Key::Down | Key::Char('n') => {
                self.move_down();
                self.refresh();
            }
            Key::Left => self.goto_grand_parent(),
            Key::Right => self.goto_selected(),
            Key::Char('r') => self.reverse_sort(),
            Key::Char('s') => self.cycle_sort(),
            Key::Char('k') => self.select_next_mtime(),
            Key::Char('K') => self.select_prev_mtime(),
            Key::Char('d') => self.toggle_dirs_first(),
            Key::Char('!') => self.exec_cmd(),
            _ => {
                self.bad(Event::Key(key));
            }
        }
    }
}
