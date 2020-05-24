use splay_tree::set::{SplaySet, VecLike, VecLikeMut};

use crate::files::File;
use std::sync::RwLockWriteGuard;

pub struct FileIter<'a> {
    files: &'a mut SplaySet<File>,
    current: Option<&'a File>,
    filter_fn: Box<dyn Fn(&File) -> bool>,
    done: bool,
    n: usize,
}

impl<'a> FileIter<'a> {
    pub fn new(files: &'a mut SplaySet<File>, filter_fn: Box<dyn Fn(&File) -> bool>) -> Self {
        let mut iter = FileIter {
            files: files,
            current: None,
            filter_fn,
            done: false,
            n: 0,
        };

        let file = iter.find_first_nonhidden();
        iter.current = file;
        iter
    }

    pub fn start_from(mut self, file: &'a File) -> Self {
        // let mut first = file.clone();

        // while let Some(current) = self.files.find_less(&first) {
        //     if (self.filter_fn)(current) == true {
        //         first = current.clone();
        //         break;
        //     } else {
        //         first = current.clone();
        //     }
        // }
        dbg!("STARTING FROM");
        dbg!(&file);
        self.current = Some(file);
        self
    }

    pub fn seek_back(mut self, n: usize) -> Self {
        if n > 0 {
            dbg!(n);
            dbg!(&self.current);
            self.nth_back(n - 1);
            dbg!(&self.current);
        }
        self
    }

    fn find_prev_nonhidden(&mut self, of: &'a File) -> Option<&'a File> {
        let mut prev = of;
        while let Some(current) = self.files.find_less(&prev).extend() {
            prev = current;
            if (self.filter_fn)(current) == true {
                break;
            }
        }

        if of != prev {
            Some(prev)
        } else {
            None
        }
    }

    fn find_next_nonhidden(&mut self, of: &'a File) -> Option<&'a File> {
        let mut next = of;
        while let Some(current) = self.files.find_upper_bound(&next).extend() {
            next = current;
            if (self.filter_fn)(current) == true {
                break;
            }
        }

        if of != next {
            Some(next)
        } else {
            None
        }
    }

    fn find_first_nonhidden(&mut self) -> Option<&'a File> {
        self.files.smallest().extend().map(|f| {
            let mut first = f;
            while let Some(current) = self.files.find_upper_bound(&first).extend() {
                first = current;
                if (self.filter_fn)(current) == true {
                    break;
                }
            }
            first
        })
    }
}

impl<'a> Iterator for FileIter<'a> {
    type Item = &'a File;

    fn next(&mut self) -> Option<&'a File> {
        // if self.done || self.n > self.files.len() {
        //     self.done = true;
        //     return None;
        // }

        if !self.done && self.current.is_none() {
            self.current = self.find_first_nonhidden();
        }

        self.current.take().map(|f| {
            self.find_next_nonhidden(f)
                .map(|f| self.current = Some(f))
                .or_else(|| {
                    self.done = true;
                    None
                });
            f
        })
    }
}

impl<'a> DoubleEndedIterator for FileIter<'a> {
    fn next_back(&mut self) -> Option<&'a File> {
        // dbg!(&self.current);
        // self.current
        //     .clone()
        //     .as_ref()
        //     .and_then(|f| unsafe {
        //         std::mem::transmute::<_, Option<&File>>(self.files.find_less(f))
        //             .and_then(|f: &File| {
        //                 self.current = Some(f.clone());
        //                 Some(f)
        //             })
        //             .filter(|f| (self.filter_fn)(f))
        //             .or_else(|| self.next_back())
        //     })

        self.current.and_then(|f| {
            self.find_prev_nonhidden(f)
                .extend()
                .map(|f| self.current = Some(f))
                .and_then(|_| self.current)
            //     .or_else(|| { self.current = None; None });
            // self.current
        })
    }
}

pub struct FileIterMut<'a> {
    files: VecLikeMut<'a, File>,
    pos: usize,
    filter_fn: Box<dyn Fn(&File) -> bool>,
}

impl<'a, 'b: 'a> FileIterMut<'a> {
    pub fn new(files: &'b mut SplaySet<File>, filter_fn: Box<dyn Fn(&File) -> bool>) -> Self {
        FileIterMut {
            files: files.as_vec_like_mut(),
            pos: 0,
            filter_fn,
        }
    }

    pub fn set_raw_pos(mut self, pos: usize) -> Self {
        self.pos = pos;
        self
    }

    pub fn seek_back(mut self, n: usize) -> Self {
        if n > 0 {
            self.nth_back(n - 1);
        }
        self
    }

    pub fn unfiltered(mut self) -> Self {
        self.filter_fn = Box::new(|_| true);
        self
    }
}

impl<'a> Iterator for FileIterMut<'a> {
    type Item = &'a mut File;

    fn next(&mut self) -> Option<&'a mut File> {
        let file = self.files.get_mut(self.pos).map(|f| {
            let f = f as *mut _;
            let f = unsafe { &mut *f };
            f
        });
        self.pos += 1;

        match file {
            Some(file) => match (self.filter_fn)(file) {
                false => self.next(),
                true => Some(file),
            },
            None => None,
        }
    }
}

impl<'a> DoubleEndedIterator for FileIterMut<'a> {
    fn next_back(&mut self) -> Option<&'a mut File> {
        if self.pos == 0 {
            return None;
        }

        self.pos -= 1;

        let file = self.files.get_mut(self.pos).map(|f| {
            let f = f as *mut _;
            let f = unsafe { &mut *f };
            f
        });

        match file {
            Some(file) => match (self.filter_fn)(file) {
                false => self.next_back(),
                true => Some(file),
            },
            None => None,
        }
    }
}

trait LifeTimeExtended {
    fn extend<'a>(self) -> Option<&'a File>;
}

impl LifeTimeExtended for Option<&File> {
    fn extend<'a>(self) -> Option<&'a File> {
        self.and_then(|f| unsafe {
            let f = f as *const File;
            Some(&*f)
        })
    }
}

// pub struct FileIter<'a> {
//     files: &'a mut SplaySet<File>,
//     current: File,
//     filter_fn: Box<dyn Fn(&File) -> bool>,
//     done: bool,
//     n: usize,
// }

// impl<'a> FileIter<'a> {
//     pub fn new(files: &'a mut SplaySet<File>, filter_fn: Box<dyn Fn(&File) -> bool>) -> Self {
//         let mut first = files.smallest().cloned().unwrap();

//         while let Some(current) = files.find_upper_bound(&first) {
//             if (filter_fn)(current) == true {
//                 break;
//             } else {
//                 first = current.clone();
//             }
//         }

//         FileIter {
//             files: files,
//             current: first,
//             filter_fn,
//             done: false,
//             n: 0
//         }
//     }

//     pub fn start_from(mut self, file: &File) -> Self {
//         let mut first = file.clone();

//         while let Some(current) = self.files.find_less(&first) {
//             if (self.filter_fn)(current) == true {
//                 first = current.clone();
//                 break;
//             } else {
//                 first = current.clone();
//             }
//         }
//         dbg!("STARTING FROM");
//         dbg!(&file);
//         self.current = first;
//         self
//     }

//     pub fn seek_back(mut self, n: usize) -> Self {
//         if n > 0 {
//             dbg!(n);
//             dbg!(&self.current);
//             self.nth_back(n-1);
//             dbg!(&self.current);
//         }
//         self
//     }

//     fn find_prev_nonhidden(&mut self) {

//     }
// }

// impl<'a> Iterator for FileIter<'a> {
//     type Item=&'a File;

//     fn next(&mut self) -> Option<&'a File> {
//         //dbg!(&self.current);
//         self.n+=1;
//         if self.done || self.n > self.files.len() {
//             self.done = true;
//             return None;
//         }

//         self.files.find_upper_bound(&self.current)
//                   .extend()
//                   .and_then(|f| {
//                       self.current = f.clone();
//                       Some(f)
//                   })
//                   .filter(|f| (self.filter_fn)(f))
//                   .or_else(|| self.next())
//     }
// }

// impl<'a> DoubleEndedIterator for FileIter<'a> {
//     fn next_back(&mut self) -> Option<&'a File> {
//         // dbg!(&self.current);
//         // self.current
//         //     .clone()
//         //     .as_ref()
//         //     .and_then(|f| unsafe {
//         //         std::mem::transmute::<_, Option<&File>>(self.files.find_less(f))
//         //             .and_then(|f: &File| {
//         //                 self.current = Some(f.clone());
//         //                 Some(f)
//         //             })
//         //             .filter(|f| (self.filter_fn)(f))
//         //             .or_else(|| self.next_back())
//         //     })

//         self.files.find_less(&self.current)
//                   .extend()
//                   .and_then(|f| {
//                       self.current = f.clone();
//                       dbg!(&self.current.name);
//                       if (self.filter_fn)(f) == true {
//                           Some(f)
//                       } else {
//                           self.next_back()
//                       }
//                   })
//                   .or_else(|| {
//                       self.current = self.files.smallest().unwrap().clone();
//                       None
//                   })

//     }
// }

// pub struct FileIterMut<'a> {
//     files: VecLikeMut<'a, File>,
//     pos: usize,
//     filter_fn: Box<dyn Fn(&File) -> bool>,
// }

// impl<'a, 'b: 'a> FileIterMut<'a> {
//     pub fn new(files: &'b mut SplaySet<File>, filter_fn: Box<dyn Fn(&File) -> bool>) -> Self {
//         FileIterMut {
//             files: files.as_vec_like_mut(),
//             pos: 0,
//             filter_fn
//         }
//     }

//     pub fn set_raw_pos(mut self, pos: usize) -> Self {
//         self.pos = pos;
//         self
//     }

//     pub fn seek_back(mut self, n: usize) -> Self {
//         if n > 0 {
//             self.nth_back(n-1);
//         }
//         self
//     }

//     pub fn unfiltered(mut self) -> Self {
//         self.filter_fn = Box::new(|_| true);
//         self
//     }
// }

// impl<'a> Iterator for FileIterMut<'a>  {
//     type Item=&'a mut File;

//     fn next(&mut self) -> Option<&'a mut File> {
//         let file = self.files.get_mut(self.pos)
//                              .map(|f| {
//                                  let f = f as *mut _;
//                                  let f = unsafe {
//                                      &mut *f
//                                  };
//                                  f
//                              });
//         self.pos += 1;

//         match file {
//             Some(file) => {
//                 match (self.filter_fn)(file) {
//                     false => self.next(),
//                     true => Some(file)
//                 }
//             }
//             None => None
//         }
//     }
// }

// impl<'a> DoubleEndedIterator for FileIterMut<'a> {
//     fn next_back(&mut self) -> Option<&'a mut File> {
//         if self.pos == 0 {
//             return None;
//         }

//         self.pos -= 1;

//         let file = self.files.get_mut(self.pos)
//                              .map(|f| {
//                                  let f = f as *mut _;
//                                  let f = unsafe {
//                                      &mut *f
//                                  };
//                                  f
//                              });

//         match file {
//             Some(file) => {
//                 match (self.filter_fn)(file) {
//                     false => self.next_back(),
//                     true => Some(file)
//                 }
//             }
//             None => None
//         }
//     }
// }

// trait LifeTimeExtended {
//     fn extend<'a>(self) -> Option<&'a File>;
// }

// impl LifeTimeExtended for Option<&File> {
//     fn extend<'a>(self) -> Option<&'a File> {
//         self.and_then(|f| {
//             unsafe {
//                 let f = f as *const File;
//                 Some(&*f)
//             }
//         })
//     }
// }
