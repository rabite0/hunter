use mime_guess::Mime;
use termion::event::Key;

use async_value::Async;

use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    mpsc::Sender,
};
use std::ffi::OsString;
use std::str::FromStr;


use crate::fail::{HResult, HError};
use crate::widget::{Widget, WidgetCore, Events};
use crate::foldview::{Foldable, FoldableWidgetExt};
use crate::listview::ListView;
use crate::proclist::ProcView;
use crate::files::File;
use crate::paths;
use crate::term;
use crate::term::ScreenExt;


pub type QuickActionView = ListView<Vec<QuickActions>>;

impl FoldableWidgetExt for ListView<Vec<QuickActions>> {
    fn on_refresh(&mut self) -> HResult<()> {
        for action in self.content.iter_mut() {
            action.actions.pull_async().ok();
            let content = action.actions
                .get()
                .map(|actions| {
                    actions
                        .iter()
                        .map(|action| {
                            let queries = action.queries
                                .iter()
                                .map(|q| String::from(":") + &q.to_string() + "?")
                                .collect::<String>();
                            format!("{}{}",
                                    crate::term::highlight_color(),
                                    action.title.clone() + &queries + "\n")
                        })
                        .collect::<String>()
                });

            if let Ok(content) = content {
                let content = format!("{}{}\n{}",
                                      crate::term::status_bg(),
                                      action.description, content);
                let lines = content.lines().count();
                action.content = Some(content);
                action.lines = lines;
            }
        }


        Ok(())
    }

    fn render_header(&self) -> HResult<String> {
        let mime = &self.content.get(0)?.mime;
        Ok(format!("QuickActions for MIME: {}", mime))
    }

    fn on_key(&mut self, key: Key) -> HResult<()> {
        match key {
            Key::Char('a') |
            Key::Ctrl('c') |
            Key::Esc       |
            Key::Char('b') => HError::popup_finnished()?,
            // undefined key causes parent to handle move up/down
            Key::Char('n') => HError::undefined_key(key)?,
            Key::Char('p') => HError::undefined_key(key)?,
            Key::Char('f') => self.run_action(None),

            key @ Key::Char(_) => {
                let chr = match key {
                    Key::Char(key) => key,
                    // some other key that becomes None with letter_to_num()
                    _ => 'x'
                };

                let num = self.letter_to_num(chr);

                if let Some(num) = num {
                    // only select the action at first, to prevent accidents
                    if self.get_selection() != num {
                        self.set_selection(num);
                        return Ok(());
                    // activate the action the second time the key is pressed
                    } else {
                        if self.is_description_selected() {
                            self.toggle_fold()?;
                        } else {
                            self.run_action(Some(num))?;
                            HError::popup_finnished()?
                        }
                    }
                }

                // Was a valid key, but not used, don't handle at parent
                return Ok(());
            }
            _ => HError::undefined_key(key)?
        }?;

        HError::popup_finnished()?
    }

    fn render(&self) -> Vec<String> {
        let (xsize, _) = self.core.coordinates.size_u();
        self.content
            .iter()
            .fold(Vec::<String>::new(), |mut acc, atype| {
                let mut alist = atype.render()
                    .iter()
                    .enumerate()
                    .map(|(i, line)| {
                         term::sized_string_u(&format!("[{}]: {}",
                                                       self.num_to_letter(acc.len() + i),
                                                       line),
                                              xsize)
                    })
                    .collect::<Vec<_>>();

                acc.append(&mut alist);
                acc
            })
    }
}


impl ListView<Vec<QuickActions>> {
    fn render(&self) -> Vec<String> {
        vec![]
    }

    fn is_description_selected(&self) -> bool {
        if let Some(current_fold) = self.current_fold() {
            let fold_start_pos = self.fold_start_pos(current_fold);
            let selection = self.get_selection();
            selection == fold_start_pos
        } else {
            false
        }
    }

    fn run_action(&mut self, num: Option<usize>) -> HResult<()> {
        num.map(|num| self.set_selection(num));

        let current_fold = self.current_fold()?;
        let fold_start_pos = self.fold_start_pos(current_fold);
        let selection = self.get_selection();
        let selected_action_index = selection - fold_start_pos;

        self.content[current_fold]
            .actions
            // -1 because fold description takes one slot
            .get()?[selected_action_index-1]
            .run(self.content[0].files.clone(),
                 &self.core,
                 self.content[0].proc_view.clone())?;

        self.core.screen()?.clear()?;
        Ok(())
    }

    fn num_to_letter(&self, num: usize) -> String {
        if num > 9 && num < (CHARS.chars().count() + 10) {
            // subtract number keys
            CHARS.chars()
                .skip(num-10)
                .take(1)
                .collect()
        } else if num < 10{
            format!("{}", num)
        } else {
            String::from("..")
        }

    }

    fn letter_to_num(&self, letter: char) -> Option<usize> {
        CHARS.chars()
            .position(|ch| ch == letter)
            .map(|pos| pos + 10)
            .or_else(||
                     format!("{}", letter)
                     .parse::<usize>()
                     .ok())
    }
}

// shouldn't contain keys used for navigation/activation
static CHARS: &str = "bcdeghijklmoqrstuvxyz";

impl QuickActions {
    pub fn new(files: Vec<File>,
               mime: mime::Mime,
               subpath: &str,
               description: String,
               sender: Sender<Events>,
               proc_view: Arc<Mutex<ProcView>>) -> HResult<QuickActions> {
        let mut actions = files.get_actions(mime.clone(), subpath.to_string());

        actions.on_ready(move |_,_| {
            sender.send(Events::WidgetReady).ok();
            Ok(())
        })?;

        actions.run()?;


        Ok(QuickActions {
            description: description,
            files: files,
            mime: mime,
            content: None,
            lines: 1,
            folded: false,
            actions: actions,
            proc_view: proc_view
        })
    }
}

pub fn open(files: Vec<File>,
           sender: Sender<Events>,
           core: WidgetCore,
           proc_view: Arc<Mutex<ProcView>>) -> HResult<()> {
    let mime  = files.common_mime()
        .unwrap_or_else(|| Mime::from_str("*/").unwrap());


    let act = QuickActions::new(files.clone(),
                                mime.clone(),
                                "",
                                String::from("UniActions"),
                                sender.clone(),
                                proc_view.clone()).unwrap();

    let mut action_view: QuickActionView = ListView::new(&core, vec![]);
    action_view.content = vec![act];


    let subdir = mime.type_().as_str();
    let act_base = QuickActions::new(files.clone(),
                                     mime.clone(),
                                     subdir,
                                     String::from("BaseActions"),
                                     sender.clone(),
                                     proc_view.clone());

    let subdir = &format!("{}/{}",
                          mime.type_().as_str(),
                          mime.subtype().as_str());
    let act_sub = QuickActions::new(files,
                                 mime.clone(),
                                 subdir,
                                 String::from("SubActions"),
                                 sender,
                                 proc_view);

    act_base.map(|act| action_view.content.push(act)).ok();
    act_sub.map(|act| action_view.content.push(act)).ok();

    action_view.popup()
}


#[derive(Debug)]
pub struct QuickActions {
    description: String,
    files: Vec<File>,
    mime: mime::Mime,
    content: Option<String>,
    lines: usize,
    folded: bool,
    actions: Async<Vec<QuickAction>>,
    proc_view: Arc<Mutex<ProcView>>
}

impl Foldable for QuickActions {
    fn description(&self) -> &str {
        &self.description
    }

    fn render_description(&self) -> String {
        format!("{}{}",
                term::status_bg(),
                &self.description)
    }

    fn content(&self) -> Option<&String> {
        self.content.as_ref()
    }

    fn lines(&self) -> usize {
        if self.folded
        { 1 } else
        { self.lines }
    }

    fn toggle_fold(&mut self) {
        self.folded = !self.folded;
    }

    fn is_folded(&self) -> bool {
        self.folded
    }
}





#[derive(Debug)]
pub struct QuickAction {
    path: PathBuf,
    title: String,
    queries: Vec<String>,
    sync: bool,
    mime: mime::Mime
}

impl QuickAction {
    fn new(path: PathBuf, mime: mime::Mime) -> QuickAction {
        let title = path.get_title();
        let queries = path.get_queries();
        let sync = path.get_sync();

        QuickAction {
            path,
            title,
            queries,
            sync,
            mime
        }
    }

    fn run(&self,
           files: Vec<File>,
           core: &WidgetCore,
           proc_view: Arc<Mutex<ProcView>>) -> HResult<()> {

        let answers = self.queries
            .iter()
            .fold(Ok(vec![]), |mut acc, query| {
                // If error occured/input was cancelled just skip querying
                // Turn into try_fold?
                if acc.is_err() { return acc; }

                match core.minibuffer(query) {
                    Err(HError::MiniBufferEmptyInput) => {
                        acc.as_mut()
                            .map(|acc| acc.push((OsString::from(query),
                                                 OsString::from(""))))
                            .ok();
                        acc
                    }
                    Ok(input) => {
                        acc.as_mut()
                            .map(|acc| acc.push((OsString::from(query),
                                                 OsString::from(input))))
                            .ok();
                        acc
                    }
                    Err(err) => Err(err)
                }
            })?;

        let cwd = files.get(0)?.parent_as_file()?;

        let files = files.iter()
            .map(|f| OsString::from(&f.path))
            .collect();



        if self.sync {
            std::process::Command::new(&self.path)
                .args(files)
                .envs(answers)
                .spawn()?
                .wait()?;
            Ok(())
        } else {
            let cmd = crate::proclist::Cmd {
                cmd: std::ffi::OsString::from(&self.path),
                args: Some(files),
                vars: Some(answers),
                short_cmd: None,
                cwd: cwd,
                cwd_files: None,
                tab_files: None,
                tab_paths: None
            };

            proc_view
                .lock()
                .map(|mut proc_view| {
                    proc_view.run_proc_raw(cmd)
                })??;

            Ok(())
        }
    }
}



pub trait QuickFiles {
    fn common_mime(&self) -> Option<Mime>;
    fn get_actions(&self, mime: mime::Mime, subpath: String) -> Async<Vec<QuickAction>>;
}

impl QuickFiles for Vec<File> {
    // Compute the most specific MIME shared by all files
    fn common_mime(&self) -> Option<Mime> {
        let first_mime = self
            .get(0)?
            .get_mime();


        self.iter()
            .fold(first_mime, |common_mime, file| {
                let cur_mime = file.get_mime();

                if &cur_mime == &common_mime {
                    cur_mime
                } else {

                    // MIMEs differ, find common base

                     match (cur_mime, common_mime) {
                        (Some(cur_mime), Some(common_mime)) => {
                            // Differ in suffix?

                            if cur_mime.type_() == common_mime.type_()
                                && cur_mime.subtype() == common_mime.subtype()
                            {
                                Mime::from_str(&format!("{}/{}",
                                                        cur_mime.type_().as_str(),
                                                        cur_mime.subtype().as_str()))
                                               .ok()
                            }

                            // Differ in subtype?

                            else if cur_mime.type_() == common_mime.type_() {
                                Mime::from_str(&format!("{}/",
                                                        cur_mime.type_()
                                                        .as_str()))
                                     .ok()

                                // Completely different MIME types

                            } else {
                                None
                            }
                        }
                         _ => None
                     }
                }
            })
    }

    fn get_actions(&self, mime: mime::Mime, subpath: String) -> Async<Vec<QuickAction>> {
        Async::new(move |_| {
            let mut apath = paths::actions_path()?;
            apath.push(subpath);
            Ok(std::fs::read_dir(apath)?
               .filter_map(|file| {
                   let path = file.ok()?.path();
                   if !path.is_dir() {
                       Some(QuickAction::new(path, mime.clone()))
                   } else {
                       None
                   }
               }).collect())
        })
    }
}


pub trait QuickPath {
    fn get_title(&self) -> String;
    fn get_queries(&self) -> Vec<String>;
    fn get_sync(&self) -> bool;
}

impl QuickPath for PathBuf {
    fn get_title(&self) -> String {
        self.file_stem()
            .map(|stem| stem
                 .to_string_lossy()
                 .splitn(2, "?")
                 .collect::<Vec<&str>>()[0]
                 .to_string())
            .unwrap_or_else(|| String::from("Filename missing!"))
    }

    fn get_queries(&self) -> Vec<String> {
        self.file_stem()
            .map(|stem| stem
                 .to_string_lossy()
                 .split("?")
                 .collect::<Vec<&str>>()
                 .iter()
                 .skip(1)
                 // Remove ! in queries from sync actions
                 .map(|q| q.trim_end_matches("!").to_string())
                 .collect())
            .unwrap_or_else(|| vec![])
    }

    fn get_sync(&self) -> bool {
        self.file_stem()
            .map(|stem| stem
                 .to_string_lossy()
                 .ends_with("!"))
            .unwrap_or(false)
    }
}
