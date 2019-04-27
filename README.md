hunter
======

![hunter](https://raw.githubusercontent.com/rabite0/hunter/master/docs/hunter.png)

hunter is a fast and lag-free file browser/manager for the terminal. It features a heavily asynchronous and multi-threaded design and all disk IO happens off the main thread in a non-blocking fashion, so that hunter will always stay responsive, even under heavy load on a slow spinning rust disk, even with all the previews enabled.

It's heavily inspired by the excellent ranger, but a little more Emacs-flavoured, and written in Rust to make sure it starts up quickly and to take advantage of its strong guarantees around concurrency. It's so fast I actually built in animations for some parts as a joke, but in fact it turned out to look really nice and makes it look much smoother. YMMV, of course, and this can be disabled.

Most things you would expect are implemented, among them tabs, bookmarks (with ranger-import), search/filter, previews of files/directories (including size information in previewed directories), a minibuffer at the bottom with file name completion, multi file selection, etc., etc. There are also a few original ideas, especially around subprocess handling. The process viewer actually shows the output of started subprocesses, their pids and exit codes, notifies on new output and process completion. It's somewhat of a primitive TUI shell. File names are handled using raw OsString, so there is no file it can't handle, no matter what garbage the name contains. It also sets the tmux/terminal title to the current directory on supported terminals.

To speed up the loading of directories metadata in the preview/backview is only loaded for files you can see, except in the main view. Still, metadata is also loaded asynchronously, so you can sometimes see it updating file listings while browsing through your files. I think this is better than waiting though :).

Technically hunter is not a file "manager" itself. It has no built in primitives for file manipulation like delete, rename, move, and so on. Instead it relies on its easy and extensive integration with the standard cli tools to do its job. For that purpose there are various file name/path substitution patterns and an auto-completing for executables you want to run.

This is a young project and probably (definitely) has some bugs and edge cases. It hasn't been tested on a lot of terminals, but at least alacritty, kitty and urxvt work fine. It should work on most Unix-flavoured systems supported by Rust, but was only tested on GNU/Linux. I haven't lost any files so far, at least.

A big thanks to ranger and its developers. Without its inspiration this wouldn't have been possible. hunter is not a drop-in replacement and doesn't cover every use-care, especially if you're into advanced customization, since hunter has basically none unless you modify the code, but if you just need fast above all else it might be a good choice.

## Features:
* Lag-free architecture, always responsive
* Asynchronous multi-threaded IO
* Tabs
* Multi-file selection
* Enter directories/select files using external command like fzf
* ranger import for bookmarks/tags
* minibuffer with completion and filename/selection/tab/directory substitution
* subprocess viewer that shows output of started subprocesses
* exit and cd into last directory and put selected files into shell variables
* slide up animation for previews for a smoother experience (configurable)
* fffast

## Known to work on:

* GNU/Linux
* macOS
* Windows (WSL)

If it works on a system not mentioned here, please open an issue. Also feel free to open an issue if it doesn't work on your system, if it's reasonably Unix-like.

## PREREQUISITES:

* gcc
* libmagic-dev

### Debian/Ubuntu

* ```apt install gcc libmagic-dev```

## INSTALLATION:

### Build with cargo

```
cargo install hunter
```


### Build from source

```
// Clone the git repo 
git clone https://github.com/rabite0/hunter.git 

// Go into the repos directory
cd {source_dir}/hunter/

// Note: if you want vim bindings you will have to switch to the evil branch.
// git checkout evil

// Build
cargo build --release
```

### 

This will create a binary in {source_dir}/hunter/target/release/hunter. If you want to call this like any other terminal app you have to put this binary in your path. 

```ln -s {source_dir}/hunter/target/release/hunter /usr/bin/local/hunter```

This will link the the binary to your local bin file. You can call hunter from anywhere. To update repeat the process, but you will **not** need to redo the binary link. 

## NOTE:
hunter uses [ranger's rifle](https://github.com/ranger/ranger/blob/master/ranger/ext/rifle.py) to open files if rifle is in your $PATH. If it can't find rifle it uses xdg-open. It also uses ranger's scope.sh to generate previews for non-text files. A slightly modified version is included in the "extra" directory. Put it in your $PATH somewhere if you want previews for non-text files.

## Configuration
hunter reads $XDG_CONFIG_HOME/hunter/config at startup. On macOS it reads $HOME/Library/Preferences/hunter/config, according to Apple guidelines, although this might change. There are two options, which can be set. The configuration file is read asynchronously, so if it's not read by the time hunter starts drawing you will see its default configuration until the config file is read. Options can be set like this (default config):

```
animation=on
show_hidden=off
select_cmd=find -type f | fzf -m
cd_cmd=find -type d | fzf
```

## Drop into hunter cwd on quit
To change the directory of your shell when quitting hunter with Q you need to source extra/hunter_cd.sh, which is a wrapper that runs hunter and checks for ~/.hunter_cwd after hunter exits and cd's into the contained directory if it exists.

## Filename Substitution
| Pattern   | Substituted with        |
|-----------|:------------------------|
| $s        | selected file(s)        |
| $n        | tab directory           |
| $ns       | selected files in tab   |


Keybindings:
============

## holy mode
By default hunter uses vi-style keybindings. If you use a QWERTY-like keyboard layout this is probably what you want. Most people will want this, so I maed it the default. If you have a different keyboard layout this might not be the best choice. The holy-branch changes the movement keys to the emacs keybindings, which is more ergonomic on e.g. Colemak.

## Main view:

| Key                 | Action                             |
| ------------------- | :--------------------------------- |
| j/k (holy: n/p)     | move down/up                       |
| J/K (holy: N/P)     | 5x move down/5x move up            |
| <                   | move to top                        |
| >                   | move to bottom                     |
| l/h (holy: f/b)     | open/go back                       |
| S                   | search file                        |
| Alt(s)              | search next                        |
| Alt(S)              | search prev                        |
| Ctrl(f)             | filter                             |
| space               | multi select file                  |
| Alt(space)          | select with external program       |
| v                   | invert selections                  |
| t                   | toggle tag                         |
| h                   | toggle show hidden                 |
| r                   | reverse sort                       |
| s                   | cycle sort (name/size/mtime)       |
| K                   | select next by mtime               |
| k                   | select prev by mtime               |
| d                   | toggle dirs first                  |
| /                   | turbo cd                           |
| Alt(/)              | enter dir with external program    |
| Q                   | quit with dir/selections           |
| L                   | run in background                  |
| ~                   | goto prev cwd                      |
| `                   | goto bookmark                      |
| m                   | add bookmark                       |
| w                   | show processes                     |
| g holy(l)           | show log                           |
| z                   | open subshell in cwd               |
| c                   | toggle columns                     |
| F(n)                | switch to tab                      |



## Keybindings in bookmark popup:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|(key)                |open bookmark                     |
|`                    |goto last cwd                     |
|Ctrl(c)              |cancel                            |
|Alt(key)             |delete bookmark                   |

## Keybindings in process viewer:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|w                    |close process viewer              |
|d                    |remove process                    |
|k                    |kill process                      |
|k holy(p)            |move up                           |
|j holy(n)            |move down                         |
|f                    |toggle follow output              |
|Ctrl(j) holy(Ctrl(n) |scroll output down                |
|Ctrl(k) holy(Ctrl(p) |scroll output up                  |
|Ctrl(v)              |page down                         |
|Alt(v)               |page up                           |
|<                    |scroll to bottom                  |
|>                    |scroll to top                     |


## Keybindings in minibuffer:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|Esc/Ctrl(c)          |cancel input                      |
|Tab                  |complete                          |
|F(n)                 |insert tab substitution           |
|Ctrl(d)              |delete char                       |
|Ctrl(b)              |move cursor left                  |
|Ctrl(f)              |move cursor right                 |
|Ctrl(p)/Alt(p)       |history up                        |
|Ctrl(n)/Alt(n)       |history down                      |
|Ctrl(u)              |clear line                        |
|Ctrl(h)              |delete word                       |
|Ctrl(a)              |move cursor to beginning          |
|Ctrl(e)              |move cursor to end                |
