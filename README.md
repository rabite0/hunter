hunter
======

![hunter](https://raw.githubusercontent.com/rabite0/hunter/master/docs/hunter.png)

hunter is a fast and lag-free file browser/manager for the terminal. It features a heavily asychronous and multi-threaded design and all disk IO happens off the main thread in a non-blocking fashion, so that hunter will always stay responsive, even under heavy load on a slow spinning rust disk, even with all the previews enabled.

It's heavily inspired by the excellent ranger, but a little more Emacs-flavoured, and written in Rust to make sure it starts up quickly and to take advantage of its strong guarantees around concurrency. It's so fast I actually built in animations for some parts as a joke, but in fact it turned out to look really nice and makes it look much smoother. YMMV, of course, and this can be disabled.

Most things you would expect are implementend, among them tabs, bookmarks (with ranger-import), search/filter, previews of files/directories (including size information in previewed directories), a minibuffer at the bottom with file name completion, multi file selection, etc., etc. There are also a few original ideas, especially around subprocess handling. The process viewer actually shows the output of started subprocesses, their pids and exit codes, notifies on new output and process completion. It's somewhat of a primitive TUI shell. File names are handled using raw OsString, so there is no file it can't handle, no matter what garbage the name contains. It also sets the tmux/terminal title to the current directory on supported terminals.

To speed up the loading of direcories metadata in the preview/backview is only loaded for files you can see, except in the main view. Still, metadata is also loaded asynchronously, so you can sometimes see it updating file listings while browsing through your files. I think this is better than waiting though :).

Technically hunter is not a file "manager" itself. It has no built in primitives for file manipulation like delete, rename, move, and so on. Instead it relies on its easy and extensive integration with the standard cli tools to do its job. For that purpose there are various file name/path substitution patterns and an auto-completing for executables you want to run.

This is a young project and probably (definitely) has some bugs and edge cases. It hasn't been tested on a lot of terminals, but at least alacritty, kitty and urxvt work fine. It should work on most Unix-flavoured systems supported by Rust, but was only tested on GNU/Linux. I haven't lost any files so far, at least.

A big thanks to ranger and its developers. Without its inspiration this wouldn't have been possible. hunter not a drop-in replacement and doesn't cover every use-care, especially if you're into advanced customization, since hunter has basically none unless you modify the code, but if you just need fast above all else it might be a good coice.

## Features:
* Lag-free architecture, always responsive
* Asynchronous multi-threaded IO
* Tabs
* Multi-file selection
* ranger import for bookmarks/tags
* minibuffer with completion and filename/selection/tab/direcory substitution
* subprocess viewer that shows output of started subprocesses
* exit and cd into last directory and put seleceted files into shell variables
* slide up animation for previews for a smoother experience (configurable)
* fffast

## NOTE:
hunter uses ranger's rifle to open files if rifle is in your $PATH. If it can't find rifle it uses xdg-open. It also uses ranger's scope.sh to generate previews for non-text files. A slightly modified version is included in the "extra" directory. Put it in your $PATH somewhere if you want previews for non-text files.


## Configuration
hunter reads $XDG_CONFIG_HOME/hunter/config at startup. There are two options, which can be set. The configuration file is read asynchronously, so if it's not read by the time hunter starts drawing you will see its default configuration until the config file is read. Options can be set like this (default config):

animation=on
show_hidden=off



Keybindings:
============

## evil mode
By default hunter uses emacs style keybindings. If you use a QWERTY-like keyboard layout this is probably not what you want. In that case use the "evil" branch which remaps movement keys to vi-style.

## Main view:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|n/p (evil: n/j)      |move up/down                      |
|N/P (evil: N/J)      |5x move up/5x move down           |
|<                    |move to top                       |
|>                    |mve to bottom                     |
|f/b (evil: h/l)      |enter (run executable)            |
|S                    |search file                       |
|Alt(s)               |search next                       |
|Alt(S)               |search prev                       |
|Ctrl(f)              |filer                             |
|space                |multi select file                 |
|v                    |invert selections                 |
|t                    |toggle tag                        |
|h                    |toggle show hidden                |
|r                    |reverse sort                      |
|s                    |cycle sort (name/size/mtime)      |
|K                    |select next by mtime              |
|k                    |select prev by mtime              |
|d                    |toggle dirs first                 |
|/                    |turbo cd                          |
|Q                    |quit with dir/selections          |
|F                    |start executabe in background     |
|-                    |goto prev cwd                     |
|`                    |goto bookmark                     |
|m                    |add bookmark                      |
|w                    |show processes                    |
|l evil(L)            |show log                          |
|z                    |open subshell in cwd              |
|c                    |toggle columns                    |
|F(n)                 |switch to tab                     |



## Keybindings in bookmark popup:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|(key)                |open bookmark                     |
|`                    |goto last cwd                     |
|Ctrl(c)              |cancel                            |
|Alt(key)           |delete bookmark                   |

## Keybindings in process viewer:

| Key                 | Action                           |
| ------------------- |:---------------------------------|
|w                    |close process viewer              |
|d                    |remove process                    |
|k                    |kill process                      |
|p evil(k)            |move up                           |
|n evil(j)            |move down                         |
|f                    |toggle follow outupt              |
|Ctrl(n) evil(Ctrl(j) |scroll output down                |
|Ctrl(p) evil(Ctrl(k) |scroll output up                  |
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
|Ctrl(b)              |mvoe cursor left                  |
|Ctrl(f)              |move cursor right                 |
|Ctrl(p)/Alt(p)       |history up                        |
|Ctrl(n)/Alt(n)       |history down                      |
|Ctrl(u)              |clear line                        |
|Ctrl(h)              |delete word                       |
|Ctrl(a)              |move cursor to beginning          |
|Ctrl(e)              |move cursor to end                |
