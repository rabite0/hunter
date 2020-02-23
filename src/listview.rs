use std::fmt::Debug;
use std::path::PathBuf;

use termion::event::Key;
use unicode_width::UnicodeWidthStr;

use async_value::Stale;

use crate::files::{File, Files};
use crate::fail::{HResult, HError, ErrorLog};
use crate::term;
use crate::widget::{Widget, WidgetCore};
use crate::dirty::Dirtyable;
use crate::fscache::FsCache;


pub trait Listable {
    type Item: Debug + PartialEq + Default;
    fn len(&self) -> usize;
    fn render(&self) -> Vec<String>;
    fn render_header(&self) -> HResult<String> { Ok("".to_string()) }
    fn render_footer(&self) -> HResult<String> { Ok("".to_string()) }
    fn on_new(&mut self) -> HResult<()> { Ok(()) }
    fn on_refresh(&mut self) -> HResult<()> { Ok(()) }
    fn on_key(&mut self, _key: Key) -> HResult<()> { Ok(()) }
}

use crate::keybind::{Acting, Bindings, FileListAction, Movement};


impl Acting for ListView<Files> {
    type Action=FileListAction;

    fn search_in(&self) -> Bindings<Self::Action> {
        self.core.config().keybinds.filelist
    }

    fn movement(&mut self, movement: &Movement) -> HResult<()> {
        use Movement::*;

        let pos = self.get_selection();

        match movement {
            Up(n) => { for _ in 0..*n { self.move_up(); }; self.refresh()?; }
            Down(n) => { for _ in 0..*n { self.move_down(); }; self.refresh()?; }
            PageUp => self.page_up(),
            PageDown => self.page_down(),
            Top => self.move_top(),
            Bottom => self.move_bottom(),
            Left | Right => {}
        }

        if pos != self.get_selection() {
            self.update_selected_file(pos);
        }

        Ok(())
    }

    fn do_action(&mut self, action: &Self::Action) -> HResult<()> {
        use FileListAction::*;

        match action {
            Search => self.search_file()?,
            SearchNext => self.search_next()?,
            SearchPrev => self.search_prev()?,
            Filter => self.filter()?,
            Select => self.multi_select_file(),
            InvertSelection => self.invert_selection(),
            ClearSelection => self.clear_selections(),
            FilterSelection => self.toggle_filter_selected(),
            ToggleTag => self.toggle_tag()?,
            ToggleHidden => self.toggle_hidden(),
            ReverseSort => self.reverse_sort(),
            CycleSort => self.cycle_sort(),
            ToNextMtime => self.select_next_mtime(),
            ToPrevMtime => self.select_prev_mtime(),
            ToggleDirsFirst => self.toggle_dirs_first(),
        }

        Ok(())
    }
}

impl Listable for ListView<Files> {
    type Item = File;

    fn len(&self) -> usize {
        self.content.len()
    }

    fn render(&self)-> Vec<String> {
        self.render()
    }

    fn on_new(&mut self) -> HResult<()> {
        let show_hidden = self.core.config().show_hidden();
        self.content.show_hidden = show_hidden;
        Ok(())
    }

    fn on_refresh(&mut self) -> HResult<()> {
        if self.content.len() == 0 {
            let path = &self.content.directory.path;
            let placeholder = File::new_placeholder(&path)?;
            self.content.files.insert(placeholder);
            self.content.len = 1;
        }

        let meta_upto = self.content.meta_upto.unwrap_or(0);
        let ysize = self.core.coordinates.ysize_u();

        if  self.offset + ysize >= meta_upto {
            let sender = self.core.get_sender();
            let njobs = self.offset + ysize;

            self.content.enqueue_jobs(njobs);
            self.content.run_jobs(sender);
        }

        // self.refresh_files().log();

        // if self.content.is_dirty() {
        //     self.content.set_clean();
        //     self.core.set_dirty();
        // }

        Ok(())
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        self.do_key(key)
    }
}

#[derive(Debug, PartialEq)]
pub struct ListView<T>
where
    ListView<T>: Listable
{
    pub content: T,
    pub current_item: Option<<ListView<T> as Listable>::Item>,
    selection: usize,
    pub offset: usize,
    pub core: WidgetCore,
    seeking: bool,
    searching: Option<String>,
}

impl<T> ListView<T>
where
    ListView<T>: Widget,
    ListView<T>: Listable
{
    pub fn new(core: &WidgetCore, content: T) -> ListView<T> {
        let mut view = ListView::<T> {
            content: content,
            current_item: None,
            selection: 0,
            offset: 0,
            core: core.clone(),
            seeking: false,
            searching: None
        };
        view.on_new().log();
        view
    }

    pub fn move_up(&mut self) {
        if self.selection == 0 {
            return;
        }

        if self.selection - self.offset <= 0 {
            self.offset -= 1;
        }

        self.selection -= 1;
        self.seeking = false;
    }
    pub fn move_down(&mut self) {
        let lines = self.len();
        let y_size = self.get_coordinates().unwrap().ysize() as usize;

        if lines == 0 || self.selection == lines - 1 {
            return;
        }

        if self.selection + 1 >= y_size && self.selection + 1 - self.offset >= y_size {
            self.offset += 1;
        }

        self.selection += 1;
        self.seeking = false;
    }

    pub fn move_top(&mut self) {
        self.set_selection(0);
    }

    pub fn move_bottom(&mut self) {
        let lines = self.len();
        self.set_selection(lines - 1);
    }

    pub fn page_up(&mut self) {
        let ysize = self.get_coordinates().unwrap().ysize_u();

        for _ in 0..ysize {
            self.move_up();
        }
    }

    pub fn page_down(&mut self) {
        let ysize = self.get_coordinates().unwrap().ysize_u();

        for _ in 0..ysize {
            self.move_down();
        }
    }

    pub fn get_selection(&self) -> usize {
        self.selection
    }

    pub fn set_selection(&mut self, position: usize) {
        let ysize = self.get_coordinates().unwrap().ysize() as usize;
        let mut offset = 0;

        while position >= ysize + offset {
            offset += 1
        }

        self.offset = offset;
        self.selection = position;
    }

}

#[derive(PartialEq)]
pub enum FileSource {
    Path(File),
    Files(Files)
}


pub struct FileListBuilder {
    core: WidgetCore,
    source: FileSource,
    cache: Option<crate::fscache::FsCache>,
    selected_file: Option<File>,
    stale: Option<Stale>,
    meta_upto: usize,
    meta_all: bool,
}

impl FileListBuilder {
    pub fn new(core: WidgetCore, source: FileSource) -> Self {
        FileListBuilder {
            core: core,
            source: source,
            cache: None,
            selected_file: None,
            stale: None,
            meta_upto: 0,
            meta_all: false,
        }
    }

    pub fn select(mut self, file: impl Into<Option<File>>) -> Self {
        self.selected_file = file.into();
        self
    }

    pub fn with_cache(mut self, cache: impl Into<Option<FsCache>>) -> Self {
        self.cache = cache.into();
        self
    }

    pub fn with_stale(mut self, stale: impl Into<Option<Stale>>) -> Self {
        self.stale = stale.into();
        self
    }

    pub fn meta_upto(mut self, upto: impl Into<Option<usize>>) -> Self {
        self.meta_upto = upto.into().unwrap_or(0);
        self
    }

    pub fn meta_all(mut self) -> Self {
        self.meta_all = true;
        self
    }

    pub fn build(mut self) -> HResult<ListView<Files>> {
        use std::time::Instant;

        let now = Instant::now();

        let c = &self.cache;
        let s = self.stale.clone();
        let core = self.core;
        let cfg = core.config();
        let source = self.source;
        let selected_file = self.selected_file.take();

        // Run ticker for those nice loading animations (...)
        crate::files::start_ticking(core.get_sender());

        // Already sorted
        let nosort = match source {
            FileSource::Files(_) => true,
            _ => false
        };

        let mut files =
            match source {
                FileSource::Files(f) => Ok(f),
                FileSource::Path(f) => {
                    c.as_ref()
                     .map_or_else(| | unreachable!(),
                                  |c| s.map_or_else(| | c.get_files_sync(&f),
                                                    |s| c.get_files_sync_stale(&f, s)))
                }
            }?;

        // Check/set hidden flag and recalculate number of files if it's different
        if !files.show_hidden == cfg.show_hidden() {
            files.show_hidden = cfg.show_hidden();
            files.recalculate_len();
        }

        // TODO: Fix sorting so it works with lazy/partial sorting
        if !nosort {
            //files.sort();
        }

        let mut view = ListView::new(&core, files);

        selected_file
            .or_else(|| c.as_ref()
                     .and_then(|c| c.get_selection(&view.content.directory).ok()))
            .map(|f| view.select_file(&f));

        self.stale.map(|s| view.content.stale = Some(s));
        self.cache.map(|c| view.content.cache = Some(c));
        view.content.set_clean();
        view.core.set_clean();
        // let  len = view.content.len();
        // view.content.meta_upto = Some(len);

        crate::files::stop_ticking();

        dbg!(now.elapsed().as_millis());

        Ok(view)
    }
}

impl ListView<Files>
{
    pub fn builder(core: WidgetCore, source: FileSource) -> FileListBuilder {
        FileListBuilder::new(core, source)
    }

    pub fn update_selected_file(&mut self, oldsel: usize) {
        let newsel = self.get_selection();

        let skip =
            match newsel > oldsel {
                true => newsel - oldsel,
                false => 0
            };

        let seek_back =
            match newsel < oldsel {
                true => oldsel - newsel,
                false => 0
            };

        let oldfile = self.selected_file().clone();
        let fpos = self.content.find_file(&oldfile).unwrap_or(0);

        let file = self.content
                       .iter_files()
                       .set_raw_pos(fpos)
                       .seek_back(seek_back)
                       .nth(skip)
                       .unwrap()
                       .clone();

        let new_fpos = self.content.find_file(&file).unwrap_or(0);
        self.content.current_raw_pos = new_fpos;

        self.current_item = Some(file);
    }

    pub fn selected_file(&self) -> &File {
        self.current_item
            .as_ref()
            .or_else(|| self.content.iter_files().nth(0))
            .unwrap()
    }

    pub fn selected_file_mut(&mut self) -> &mut File {
        let raw_pos = self.content.current_raw_pos;

        let file = self.content
            .iter_files_mut()
            .set_raw_pos(raw_pos)
            .nth(0)
            .map(|f| f as *mut File);


        // Work around annoying restriction until polonius borrow checker becomes default
        // Since only ever one mutable borrow is returned this is perfectly safe
        // See also: https://github.com/rust-lang/rust/issues/21906
        match file {
            Some(file) => unsafe { return file.as_mut().unwrap() },
            None => {
                &mut self.content.directory
            }
        }
    }

    pub fn clone_selected_file(&self) -> File {
        let file = self.selected_file().clone();
        file
    }

    pub fn grand_parent(&self) -> Option<PathBuf> {
        self.selected_file().grand_parent()
    }

    pub fn select_file(&mut self, file: &File) {
        let file = file.clone();

        let posfile = self
            .content
            .iter_files()
            // .collect::<Vec<&File>>()
            // .into_par_iter()
            .enumerate()
            .find(|(_, item)| item == &&file);

        match posfile {
            Some((i, file)) => {
                self.current_item = Some(file.clone());
                self.set_selection(i);
            }
            // Something went wrong?
            None => {
                let dir = &self.content.directory.path;
                let file = file.path.clone();

                HError::wrong_directory::<()>(dir.clone(),
                                              file.clone()).log();
                let file = self.content
                                .iter_files()
                                .nth(0)
                                .cloned()
                                .or_else(|| File::new_placeholder(dir).ok())
                                .unwrap();
                self.current_item = Some(file);
                self.set_selection(0);
            }
        }
    }

    fn cycle_sort(&mut self) {
        let file = self.clone_selected_file();
        self.content.cycle_sort();
        self.content.sort();
        self.select_file(&file);
        self.refresh().log();
        self.core.show_status(&format!("Sorting by: {}", self.content.sort)).log();
    }

    fn reverse_sort(&mut self) {
        let file = self.clone_selected_file();
        self.content.reverse_sort();
        self.content.sort();
        self.select_file(&file);
        self.refresh().log();
        self.core.show_status(&format!("Reversed sorting by: {}",
                                       self.content.sort)).log();
    }

    fn select_next_mtime(&mut self) {
        let file = self.clone_selected_file();
        let dir_settings = self.content.dirs_first;
        let sort_settings = self.content.sort;

        self.content.dirs_first = false;
        self.content.sort = crate::files::SortBy::MTime;
        self.content.sort();

        self.select_file(&file);

        if self.seeking == false || self.selection + 1 >= self.content.len() {
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

        self.refresh().log();
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

        self.refresh().log();
    }

    pub fn toggle_hidden(&mut self) {
        let file = self.clone_selected_file();
        self.content.toggle_hidden();
        self.select_file(&file);
        self.core.show_status(&format!("Showing hidden files: {}",
                                        self.content.show_hidden)).log();
    }

    fn toggle_dirs_first(&mut self) {
        let file = self.clone_selected_file();
        self.content.dirs_first = !self.content.dirs_first;
        self.content.sort();
        self.select_file(&file);
        self.refresh().log();
        self.core.show_status(&format!("Direcories first: {}",
                                        self.content.dirs_first)).log();
    }

    fn multi_select_file(&mut self) {
        self.selected_file_mut().toggle_selection();

        if !self.content.filter_selected {
            let oldpos = self.get_selection();
            self.move_down();
            let newpos = self.get_selection();

            if newpos > oldpos {
                self.update_selected_file(oldpos);
            }
        } else {
            if self.content.filter_selected && self.content.len() == 0 {
                self.content.toggle_filter_selected();
                self.core.show_status("Disabled selection filter!").log();
            }

            // fix cursor when last file is unselected, etc
            self.refresh().log();
        }
    }

    pub fn invert_selection(&mut self) {
        for file in self.content.iter_files_mut() {
            file.toggle_selection();
        }

        if self.content.filter_selected && self.content.len() == 0 {
                self.content.toggle_filter_selected();
                self.core.show_status("Disabled selection filter!").log();
        }

        self.content.set_dirty();
        self.refresh().log();
    }

    pub fn clear_selections(&mut self) {
        for file in self.content.iter_files_mut() {
            file.selected = false;
        }
        self.content.set_dirty();
        self.refresh().log();
    }

    fn toggle_tag(&mut self) -> HResult<()> {
        self.selected_file_mut().toggle_tag()?;

        let oldpos = self.get_selection();
        self.move_down();
        let newpos = self.get_selection();

        if newpos > oldpos {
            self.update_selected_file(oldpos);
        }

        Ok(())
    }

    fn search_file(&mut self) -> HResult<()> {
        let selected_file = self.clone_selected_file();

        loop {
            let input = self.core.minibuffer_continuous("search");

            match input {
                Ok(input) => {
                    // Only set this, search is on-the-fly
                    self.searching = Some(input);
                }
                Err(HError::RefreshParent) => {
                    self.refresh().log();
                    continue;
                }
                Err(HError::MiniBufferEvent(ev)) => {
                    use crate::minibuffer::MiniBufferEvent::*;

                    match ev {
                        Done(_) => {}
                        NewInput(input) => {
                            let file = self.content
                                           .find_file_with_name(&input)
                                           .cloned();

                            file.map(|f| self.select_file(&f));

                            self.draw().log();

                            self.searching = Some(input);

                            continue;
                        }
                        Empty | Cancelled => {
                            self.select_file(&selected_file);
                        }
                        CycleNext => {
                            self.search_next().log();
                        }
                        CyclePrev => {
                            self.search_prev().log();
                        }
                    }
                }
                _ => {  }
            }
            break;
        }
        Ok(())
    }

    fn search_next(&mut self) -> HResult<()> {
        if self.searching.is_none() {
            self.core.show_status("No search pattern set!").log();
        }
        let prev_search = self.searching.clone()?;
        let selection = self.get_selection();

        let file = self.content
            .files
            .iter()
            .skip(selection+1)
            .find(|file| {
                if file.name.to_lowercase().contains(&prev_search) {
                    true
                } else {
                    false
                }
            }).clone();

        if let Some(file) = file {
            let file = file.clone();
            self.select_file(&file);
        } else {
            self.core.show_status("Reached last search result!").log();
        }
        Ok(())
    }

    fn search_prev(&mut self) -> HResult<()> {
        if self.searching.is_none() {
            self.core.show_status("No search pattern set!").log();
        }
        let prev_search = self.searching.clone()?;


        self.reverse_sort();

        let selection = self.get_selection();

        let file = self.content
            .files
            .iter()
            .skip(selection+1)
            .find(|file| {
                if file.name.to_lowercase().contains(&prev_search) {
                    true
                } else {
                    false
                }
            }).cloned();

        self.reverse_sort();
        self.core.clear_status().log();

        if let Some(file) = file {
            let file = file.clone();
            self.select_file(&file);
        } else {
            self.core.show_status("Reached last search result!").log();
        }

        Ok(())
    }

    pub fn set_filter(&mut self, filter: Option<String>) {
        let prev_len = self.len();
        let selected_file = self.clone_selected_file();

        self.content.set_filter(filter);

        // Only do something if filter changed something
        if self.len() != prev_len {
            self.refresh().ok();
            self.select_file(&selected_file);
            // Clear away that wouldn't get drawn over
            if self.len() < prev_len {
                self.core.clear().ok();
            }
            self.draw().ok();
        }
    }

    fn filter(&mut self) -> HResult<()> {
        use crate::minibuffer::MiniBufferEvent::*;

        let selected_file = self.selected_file().clone();
        let mut prev_filter = self.content.get_filter();

        loop {
            let filter = self.core.minibuffer_continuous("filter");

            match filter {
                Err(HError::MiniBufferEvent(event)) => {
                    match event {
                        Done(filter) => {
                            self.core.show_status(&format!("Filtering with: \"{}\"",
                                                           &filter)).log();

                            self.set_filter(Some(filter));
                        }
                        NewInput(input) => {
                            self.set_filter(Some(input.clone()));
                            continue;
                        }
                        Empty => {
                            self.set_filter(None);
                        }
                        Cancelled => {
                            self.set_filter(prev_filter.take());
                            self.select_file(&selected_file);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            break;

        }

        Ok(())
    }

    fn toggle_filter_selected(&mut self) {
        self.content.toggle_filter_selected();

        if self.content.len() == 0 {
            self.core.show_status("No files selected").log();
            self.content.toggle_filter_selected();
        }

        self.refresh().log();
    }

    fn render_line(&self, file: &File) -> String {
        let render_fn = self.render_line_fn();
        render_fn(file)
    }

    #[allow(trivial_bounds)]
    fn render_line_fn(&self) -> impl Fn(&File) -> String {
        use std::fmt::Write;
        use crate::files::FileError;

        let xsize = self.get_coordinates().unwrap().xsize();
        let config = self.core.config();
        let icons = config.icons;
        let icons_space = config.icons_space;

        move |file| -> String {
            let mut line = String::with_capacity(500);

            let (icon, icon_space) = match (icons, icons_space) {
                (true, true) => (file.icon(), " "),
                (true, false) => (file.icon(), ""),
                _ => ("", "")
            };

            let name = &file.name;

            let size = file.calculate_size();
            let (size, unit) = match size {
                Ok((size, unit)) => (size.to_string(), unit),
                Err(HError::FileError(FileError::MetaPending)) => {
                    let ticks = crate::files::tick_str();
                    (String::from(ticks), "")
                },
                Err(_) => (String::from("ERR"), "")
            };

            let (tag, tag_len) = match file.is_tagged() {
                Ok(true) => (Some(term::color_red() + "*"), 1),
                _ => (None, 0)
            };

            let tag = tag.as_ref()
                         .map(|t| t.as_str())
                         .unwrap_or("");

            let selection_color = crate::term::color_yellow();
            let (selection_gap, selection_color) = match file.is_selected() {
                true => (" ", selection_color.as_str()),
                false => ("", "")
            };

            let (link_indicator, link_indicator_len) = match file.target {
                Some(_) => (Some(format!("{}{}{}",
                                         term::color_yellow(),
                                         "--> ",
                                         term::highlight_color())), Some(4)),
                None => (None, None)
            };

            let link_indicator = link_indicator.as_ref()
                                               .map(|l| l.as_str())
                                               .unwrap_or("");
            let link_indicator_len = link_indicator_len.unwrap_or(0);

            let sized_string = term::sized_string(&name, xsize);

            let size = size.to_string();
            let size_pos = xsize - (size.len() as u16 +
                                    unit.len() as u16 +
                                    link_indicator_len as u16);

            let padding = sized_string.len() - sized_string.width_cjk();
            let padding = xsize - padding as u16;
            let padding = padding - tag_len;
            let padding = padding - icon.width() as u16;
            let padding = padding - icon_space.len() as u16;
            let padding = padding - 1;

            write!(&mut line, "{}", termion::cursor::Save).unwrap();

            match file.get_color() {
                Some(color) => write!(&mut line,
                                      "{}{}{}{}{}{}{:padding$}{}",
                                      tag,
                                      &color,
                                      selection_color,
                                      selection_gap,
                                      icon,
                                      icon_space,
                                      &sized_string,
                                      term::normal_color(),
                                      padding = padding as usize),
                _ => write!(&mut line,
                               "{}{}{}{}{}{}{:padding$}{}",
                               tag,
                               term::normal_color(),
                               selection_color,
                               selection_gap,
                               icon,
                               icon_space ,
                               &sized_string,
                               term::normal_color(),
                               padding = padding as usize),
            }.unwrap();

            write!(&mut line,
                   "{}{}{}{}{}{}",
                   termion::cursor::Restore,
                   termion::cursor::Right(size_pos),
                   link_indicator,
                   term::highlight_color(),
                   size,
                   unit).unwrap();


            line
        }
    }

    fn render(&self) -> Vec<String> {
        let render_fn = self.render_line_fn();
        let ysize = self.get_coordinates().unwrap().ysize_u();
        let files_above_selection = self.get_selection() - self.offset;
        let current_raw_pos = self.content.current_raw_pos;

        self.content
            .iter_files()
            .set_raw_pos(current_raw_pos)
            .seek_back(files_above_selection)
            .take(ysize+1)
            .map(|file| render_fn(file))
            .collect()
    }

    fn refresh_files(&mut self) -> HResult<()> {
        // let file = self.clone_selected_file();

         // if let Ok(Some(_)) = self.content.get_refresh() {
        //     // Positions might change when files are added/removed/renamed
        //     self.select_file(&file);
        //     self.content.run_jobs(self.core.get_sender());
        // }

        // if self.content.ready_to_refresh()? {
        //     self.content.process_fs_events(self.core.get_sender())?;
        // }

        Ok(())
    }
}


impl<T> Widget for ListView<T>
where
    ListView<T>: Listable
{
    fn get_core(&self) -> HResult<&WidgetCore> {
        Ok(&self.core)
    }
    fn get_core_mut(&mut self) -> HResult<&mut WidgetCore> {
        Ok(&mut self.core)
    }
    fn refresh(&mut self) -> HResult<()> {
        self.on_refresh().log();

        if self.selection >= self.len() && self.len() != 0 {
            self.selection = self.len() - 1;
        }

        Ok(())
    }

    fn render_header(&self) -> HResult<String> {
        Listable::render_header(self)
    }

    fn render_footer(&self) -> HResult<String> {
        Listable::render_footer(self)
    }

    fn get_drawlist(&self) -> HResult<String> {
        let mut output = term::reset();
        let (xpos, ypos) = self.get_coordinates().unwrap().position().position();

        let render = self.render();

        output += &render
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let mut output = term::normal_color();

                // i counts from the offset, while selection counts from 0
                if i + self.offset == self.selection {
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

        output += &self.get_redraw_empty_list(self.len())?;

        Ok(output)
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        Listable::on_key(self, key)
    }
}
