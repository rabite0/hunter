hunter
======


![hunter](https://raw.githubusercontent.com/rabite0/hunter-stuff/master/player.png)

NEW
- [**FASTER**] hunter is now *much* faster
- [Custom Keybindings] Customize keys to your liking
- [Graphics] High quality support for graphics using SIXEL/kitty protocols
- [QuickActions] Added quick action creator/customizer
- [Previews] New and improved preview customization
- [**[IRC channel](https://webchat.freenode.net/?channels=hunter)**] Problems? Bugs? Praise? Chat with us: [#hunter @ Freenode](https://webchat.freenode.net/?channels=hunter)!



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
* Optional support for previews of image(+pdf)/video/audio files using Unicode half-block drawing and SIXEL, or kitty's graphics protocol



## Known to work on:

* GNU/Linux
* macOS
* Windows (WSL)

If it works on a system not mentioned here, please open an issue. Also feel free to open an issue if it doesn't work on your system, if it's reasonably Unix-like.

## PREREQUISITES:

* gcc
* Rust-nighly compiler
* GStreamer for video/audio previews (optional)
* libsixel (optional)

### PREVIEWERS

hunter comes with definitions to enable previewing certain file types. To use this you need to install some programs first. You can also define your own. See below. Defaults are:

* bat / highlight for syntax highlighting
* bsdtar / 7z / atool  for archives
* w3m / links / elinks / lynx for html
* pdftotext / mutool for pdf or pdftoppm in graphics mode

### Debian/Ubuntu

* ```apt install gcc libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good libgstreamer-plugins-bad1.0-dev libsixel-bin```

## INSTALLATION:

Compiling hunter currently requires a nightly Rust compiler!
The easiest way to get a nightly compiler is with [rustup](https://rustup.rs/). If you have rustup installed it will automatically download and use a version that is known to work when you run cargo.

By default it will install a full-featured version with support for media-previews. You can control this using the feature flags ```img```, ```video``` and ```sixel```. These can be disabled by calling cargo with ```--no-default-features```. You can then enable image previews with ```--features=img``` and add video/audio with ```--feature=img,video```. Note that video requires img!

Note that media previews only work if hunter can find the "hunter-media" tool somewhere in $PATH!

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

### Packaging status

Fedora [Copr](https://copr.fedorainfracloud.org/coprs/atim/hunter/): `sudo dnf copr enable atim/hunter -y && sudo dnf install hunter`


## Configuration
hunter reads $XDG_CONFIG_HOME/hunter/config at startup. On macOS it simply reads ~/.config/hunter/config. There are a few options which can be set. The configuration file is read asynchronously, so if it's not read by the time hunter starts drawing you will see its default configuration until the config file is read. Options can be set like this (default config):

```
animation=on
show_hidden=off
select_cmd=find -type f | fzf -m
cd_cmd=find -type d | fzf
icons=off
ratios=20,30,49
animation_refresh_frequency=60
media_autostart=off
media_mute=off
media_previewer=hunter-media
graphics_mode=auto (other choices: kitty/sixel/unicode)
```

## Keys

Keys can be configured in ```~/.config/hunter/keys```. Some actions can be further customized with arguments. For example, you can specify a hard-coded ```Up(n)```, where n is a positive number to move up n times. This could look like ```Up(10)```=K``` to move up 10 times at once.

Some keys like F1-F12 are represented as an enum like this: ```F(n)```. You can take that number n and stick it into the ```GotoTab(n)``` action by using a placeholder binding like this: ```GotoTab(_)=F_```. That way, all F(n) keys will be bound to move to the tab number extracted from the F(n) keys.

This also works for key combinations, so you can specify ```C-_``` to bind all Ctrl-<key> combinations to some action like Delete(_) on bookmarks. To bind ```_``` itself escape it like this: ```\_```. See the default configuration for more examples.

### NOTE
hunter parses both ```M-``` and ```A-``` as Alt, so you can use whichever you like best. By default it uses ```M-```, because it came naturally and I think ```A-``` looks weird ;).

## Previews
Defining previews is easy. You just need a shell script that takes a path as first parameter and prints out what you want to see in the preview column. Put that shell script in

```$HOME/.config/hunter/previewers/definitions```

and create a symlink to it in

```$HOME/.config/hunter/previewers/```

with the extension of the file type you want to preview. Make sure the script is executable. That's it.

A graphical previewer can be created by appending ```.g``` to the name of the symlink. It should print the path to the generated image file. If you want the file deleted after display, create it in the ```/tmp/hunter-preview``` directory.

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

| FLAGS                 |                                     |
------------------------|-------------------------------------|
| -a, --animation-off   | Turn off animations                 |
| --help                | Prints help information             |
| -i, --icons           | Show icons for different file types |
| -h, --show-hidden     | Show hidden files                   |
| -u, --update-config   | Updates previewers/actions          |
| -V, --version         | Prints version information          |

### WARNING
If you made any changes to the built-in previewers/actions, those changes will be lost when using ```-u```. In that case it's better to just delete the previewer/action you want to update. On the next start hunter will reinstall the missing files automatically.


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

Note: ```_``` means any key.

## Movement:
| Action    | Key           |
|-----------|---------------|
|Up(1)      | k, Up         |
|Down(1)    | j, Down       |
|Left       | b, Left       |
|Right      | f, Right      |
|Top        | <, Home       |
|Bottom     | >, End        |
|Up(10)     | K             |
|Down(10)   | J             |
|PageUp     | C-v, PageUp   |
|PageDown   | M-v, PageDown |

## File Browser (global effects):
| Action            | Key       |
|-------------------|-----------|
| Quit              | q         |
| QuitWithDir       | Q         |
| LeftColumnDown    | ]         |
| LeftColumnUp      | [         |
| GotoHome          | ~         |
| TurboCd           | /         |
| SelectExternal    | M-Space   |
| EnterDirExternal  | M-/       |
| RunInBackground   | F         |
| GotoPrevCwd       | -         |
| ShowBookmarks     | `         |
| AddBookmark       | b         |
| ShowProcesses     | w         |
| ShowLog           | g         |
| ShowQuickActions  | a         |
| RunSubshell       | z         |
| ToggleColumns     | c         |
| ExecCmd           | !         |

## File List (affects current directory):
| Action            | Key   |
|-------------------|-------|
| Search            | C-s   |
| SearchNext        | M-s   |
| SearchPrev        | M-S   |
| Filter            | C-f   |
| Select            | Space |
| InvertSelection   | v     |
| ClearSelection    | V     |
| FilterSelection   | M-V   |
| ToggleTag         | t     |
| ToggleHidden      | h     |
| ReverseSort       | r     |
| CycleSort         | s     |
| ToNextMtime       | K     |
| ToPrevMtime       | k     |
| ToggleDirsFirst   | d     |

## Tabs
| Action     | Key      |
|------------|----------|
| NewTab     | C-t      |
| CloseTab   | C-w      |
| NextTab    | Tab      |
| PrevTab    | BackTab  |
| GotoTab(\_) | F_      |

## Media
| Action        | Key |
|---------------|-----|
| TogglePause   | M-m |
| ToggleMute    | M-M |
| SeekForward   | M-> |
| SeekBackward  | M-< |

## Bookmarks
| Action        | Key |
|---------------|-----|
| GotoLastCwd   | `   |
| Goto(\_)      | _   |
| Delete(\_)    | M-_ |

## Processes
| Action                | Key    |
|-----------------------|--------|
| Close                 | w, Esc |
| Remove                | d      |
| Kill                  | k      |
| FollowOutput          | f      |
| ScrollOutputUp        | C-p    |
| ScrollOutputDown      | C-n    |
| ScrollOutputPageUp    | C-V    |
| ScrollOutputPageDown  | C-v    |
| ScrollOutputTop       | C-<    |
| ScrollOutputBottom    | >      |

## MiniBuffer
| Action            | Key            |
|-------------------|----------------|
| InsertChar(\_)    | _              |
| InsertTab(\_)     | F_             |
| Cancel            | C-c, Esc       |
| Finish            | Enter          |
| Complete          | Tab            |
| DeleteChar        | C-d, Delete    |
| BackwardDeleteChar| Backspace      |
| CursorLeft        | C-b, Left      |
| CursorRight       | C-f, Right     |
| HistoryUp         | C-p, M-p, Up   |
| HistoryDown       | C-n, M-n, Down |
| ClearLine         | C-u            |
| DeleteWord        | C-h            |
| CursorToStart     | C-a, Home      |
| CursorToEnd       | C-e, End       |

## Folds
| Action    | Key    |
|-----------|--------|
|ToggleFold | t, Tab |

## Log
| Action  | Key    |
|---------|--------|
|Close    | g, Esc |

## QuickActions
| Action          | Key         |
|-----------------|-------------|
|Close            | a, Esc, C-a |
|SelectOrRun(\_)  | _           |
