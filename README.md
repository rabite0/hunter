hunter
======

![hunter](https://raw.githubusercontent.com/rabite0/hunter-stuff/master/player.png)

NEW
- [**[IRC channel](https://webchat.freenode.net/?channels=hunter)**] Problems? Bugs? Praise? Chat with us: [#hunter @ Freenode](https://webchat.freenode.net/?channels=hunter)!
- [Quick Actions] Run specific actions based on file type



hunter is a fast and lag-free file browser/manager for the terminal. It features a heavily asynchronous and multi-threaded design and all disk IO happens off the main thread in a non-blocking fashion, so that hunter will always stay responsive, even under heavy load on a slow spinning rust disk, even with all the previews enabled.

It's heavily inspired by the excellent ranger, but a little more Emacs-flavoured, and written in Rust to make sure it starts up quickly and to take advantage of its strong guarantees around concurrency. It's so fast I actually built in animations for some parts as a joke, but in fact it turned out to look really nice and makes it look much smoother. YMMV, of course, and this can be disabled.

Most things you would expect are implemented, among them tabs, bookmarks (with ranger-import), search/filter, previews of files/directories (including size information in previewed directories), a minibuffer at the bottom with file name completion, multi file selection, etc., etc. There are also a few original ideas, especially around subprocess handling. The process viewer actually shows the output of started subprocesses, their pids and exit codes, notifies on new output and process completion. It's somewhat of a primitive TUI shell. File names are handled using raw OsString, so there is no file it can't handle, no matter what garbage the name contains. It also sets the tmux/terminal title to the current directory on supported terminals.

To speed up the loading of directories metadata in the preview/backview is only loaded for files you can see, except in the main view. Still, metadata is also loaded asynchronously, so you can sometimes see it updating file listings while browsing through your files. I think this is better than waiting though :).

Technically hunter is not a file "manager" itself. It has no built in primitives for file manipulation like delete, rename, move, and so on. Instead it relies on its easy and extensive integration with the standard cli tools to do its job. For that purpose there are various file name/path substitution patterns and an auto-completing for executables you want to run.

It also features a "quick action" mode in which you can execute customizable actions based on the file's MIME type. These can be shell-scripts or other executables. It's possible to to make hunter ask for input before these are run. The input will be put in an environment variable for the process to use. For example, you can select a few files, run a "create archive" action and you will be asked for a name for the resulting archive. You can find a more detailed explanation below.

This is a young project and probably (definitely) has some bugs and edge cases. It hasn't been tested on a lot of terminals, but at least alacritty, kitty and urxvt work fine. It should work on most Unix-flavoured systems supported by Rust, but was only tested on GNU/Linux. I haven't lost any files so far, at least.

A big thanks to ranger and its developers. Without its inspiration this wouldn't have been possible. hunter is not a drop-in replacement and doesn't cover every use-care, especially if you're into advanced customization, since hunter has basically none unless you modify the code, but if you just need fast above all else it might be a good choice.

## Features:
* Lag-free architecture, always responsive
* Asynchronous multi-threaded IO
* Tabs
* Multi-file selection
* Customizable Quick Actions based on file type
* Enter directories/select files using external command like fzf
* ranger import for bookmarks/tags
* Minibuffer with completion and filename/selection/tab/directory substitution
* Subprocess viewer that shows output of started subprocesses
* Exit and cd into last directory and put selected files into shell variables
* Slide up animation for previews for a smoother experience (configurable)
* Can show icons with the [right fonts](https://github.com/ryanoasis/nerd-fonts)
* Optional support for previews of image/video/audio files using Unicode half-block drawing



## Known to work on:

* GNU/Linux
* macOS
* Windows (WSL)

If it works on a system not mentioned here, please open an issue. Also feel free to open an issue if it doesn't work on your system, if it's reasonably Unix-like.

## PREREQUISITES:

* gcc
* libmagic-dev
* Rust-nighly compiler
* GStreamer for video/audio previews

### Debian/Ubuntu

* ```apt install gcc libmagic-dev gstreamer1.0-devel gst-plugins-base gst-plugins-good```

## INSTALLATION:

Compiling hunter currently requires a nightly Rust compiler!
The easiest way to get a nightly compiler is with [rustup](https://rustup.rs/). If you have rustup installed it will automatically download and use a version that is known to work when you run cargo.

By default it will install a full-featured version with support for media-previews. You can control this using the feature flags ```img```, and ```video```. These can be disabled by calling cargo with ```--no-default-features```. You can then enable image previews with ```--features=img``` and add video/audio with ```--feature=img,video```. Note that video requires img!

Note that this only works if hunter can find the "preview-gen" tool somewhere in $PATH!

### Install rustup

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```


### Build with cargo

```
cargo install (--no-default-features --features=...) hunter
```


### Build from source

```
// Clone the git repo
git clone https://github.com/rabite0/hunter.git

// Go into the repos directory
cd {source_dir}/hunter/

// (Optional) Build
// cargo build --release (--no-default-features --features=...)

// Install
cargo install (--no-default-features --features=...) --path .
```

## NOTE:
hunter uses ranger's scope.sh to generate previews for non-text files. A slightly modified version is included in the "extra" directory. Put it in your $PATH somewhere if you want previews for non-text files.

## Configuration
hunter reads $XDG_CONFIG_HOME/hunter/config at startup. On macOS it simply reads ~/.config/hunter/config. There are a few options which can be set. The configuration file is read asynchronously, so if it's not read by the time hunter starts drawing you will see its default configuration until the config file is read. Options can be set like this (default config):

```
animation=on
show_hidden=off
select_cmd=find -type f | fzf -m
cd_cmd=find -type d | fzf
icons=off
media_autostart=off
media_mute=off
```

## Quick Actions
These are executables you can run by pressing ```a```. Which actions you can see depends on the MIME type of the files you have selected. If you have multiple files selected, hunter will try to use the most specific MIME type possible. For example, if you have selected a bunch of images with different types you will see actions for "image/". You can see the computed MIME type in the header bar.

There are "universal", "base-type", and "sub-type" actions. These are stored in 

```~/.config/hunter/actions/<base-type>/<sub-type>/```

Universal actions are always available. These are stored right in the "actions" directory. "Base-type" actions are stored in directories like "text", "image", "video". These correspond to the part left of the "/" in a full MIME-type like "image/png". These will be available to all "text", "image", or "video" files. This list is not exhaustive, there are a lot more base-types. In addition to that you can create a directory in those base-type directories to store "sub-type" actions, which are only available to a specific file type..

For example, if you want to define an action only available to PNG images, you can store that in 

```~/.config/hunter/actions/image/png/custom_pngcrush.sh```

You can also ask for input before those actions are run. This input will be entered through hunter's minibuffer. To ask for input append "?question" to the file name, but before the extension. hunter will then set an environment variable named after whatever you put after the question mark. You can also ask for multiple things to be entered.

For example, you could name an action 

```download_stuff?url?destination.sh```

hunter will ask for the "url" and the "destination" before running your script. The values will be available through the $url and $destination environment variables.

You can also make the action run in the foreground, so that it will take over the terminal while it runs. To do that simply append "!" to the file name before the extension. It should look like this: 

```action?query1?query2!.sh```

This will ask two questions and then run the script in the foreground until it quits.

There are a few examples in extras/actions. You can copy the whole directory into ~/.config/hunter/ and try it out.

## Startup options
You can set a few options when hunter starts. These override the configuration file. You can also tell hunter to start in a certain directory.

**USAGE: hunter [FLAGS] [path]**

| **FLAGS: **           |                                     |
------------------------|-------------------------------------|
| -a, --animation-off   | Turn off animations                 |
| --help                | Prints help information             |
| -i, --icons           | Show icons for different file types |
| -h, --show-hidden     | Show hidden files                   |
| -V, --version         | Prints version information          |


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
By default hunter uses vi-style keybindings. If you use a QWERTY-like keyboard layout this is probably what you want. Most people will want this, so I made it the default. If you have a different keyboard layout this might not be the best choice. The holy-branch changes the movement keys to the emacs keybindings, which is more ergonomic on e.g. Colemak.

## Main view:

| Key                 | Action                             |
| ------------------- | :--------------------------------- |
| j/k (holy: n/p)     | move down/up                       |
| J/K (holy: N/P)     | 5x move down/5x move up            |
| ]/[                 | move down/up on left column        |
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
| V                   | clear all selections               |
| t                   | toggle tag                         |
| h                   | toggle show hidden                 |
| r                   | reverse sort                       |
| s                   | cycle sort (name/size/mtime)       |
| K                   | select next by mtime               |
| k                   | select prev by mtime               |
| d                   | toggle dirs first                  |
| ~                   | go to $HOME                        |
| /                   | turbo cd                           |
| Alt(/)              | enter dir with external program    |
| Q                   | quit with dir/selections           |
| L                   | run in background                  |
| ~                   | goto prev cwd                      |
| `                   | goto bookmark                      |
| m                   | add bookmark                       |
| w                   | show processes                     |
| g holy(l)           | show log                           |
| a                   | show quick actions                 |
| z                   | open subshell in cwd               |
| c                   | toggle columns                     |
| F(n)                | switch to tab                      |
| Alt(m)              | toggle media pause and autoplay    |
| Alt(M)              | toggle media mute                  |
| Alt(>)              | seek media +5s                     |
| Alt(<)              | seek media -5s                     |



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
