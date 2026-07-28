if status is-interactive
# Commands to run in interactive sessions can go here
end

function bind_bang
    switch (commandline -t)[-1]
        case "!"
            commandline -t -- $history[1]
            commandline -f repaint
        case "*"
            commandline -i !
    end
end

function bind_dollar
    switch (commandline -t)[-1]
        case "!"
            commandline -f backward-delete-char history-token-search-backward
        case "*"
            commandline -i '$'
    end
end

function fish_user_key_bindings
    bind ! bind_bang
    bind '$' bind_dollar
end

abbr -a gad git add .
abbr -a --set-cursor gcm 'git commit -m "%"'

abbr -a ff fastfetch

abbr -a ytd yt-dlp

abbr -a pacman 'sudo pacman'

abbr -a restarteww '~/.config/niri/scripts/restart-eww.sh'

abbr -a --set-cursor cwp '~/.config/niri/scripts/change-wallpaper.sh ~/Pictures/Wallpapers/%'
