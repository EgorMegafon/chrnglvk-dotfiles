#!/usr/bin/env bash

ACTION="$1"
NAME="$2"
SCREEN="$3"

close_menus() {
    eww active-windows | while IFS=':' read -r id name; do
        id=$(echo "$id" | xargs)
        name=$(echo "$name" | xargs)
        if [[ "$name" == *menu* ]]; then 
            eww update reveal_"${name//-/_}"=false 2>/dev/null
        elif [[ "$name" != *bar* ]]; then
            eww close "$id"
        fi
    done

    sleep 0.3

    eww active-windows | while IFS=':' read -r id name; do
        id=$(echo "$id" | xargs)
        name=$(echo "$name" | xargs)
        if [[ "$name" != "bar" ]]; then
            eww close "$id"
            send_fifo_signal close "$name"
        fi
        
    done
}

send_fifo_signal() {
    local fifo="/tmp/eww-${2}.fifo"
    [ -p "$fifo" ] || return 0
    timeout 0.3 sh -c "printf '%s\n' '$1' > '$fifo'"
}

if [[ "$ACTION" == "close-menus" ]]; then 
    close_menus
fi

if [[ "$ACTION" == "open-menu" ]]; then 
    if eww active-windows | grep -q "^.*: ${NAME}$"; then
        close_menus
    else
        eww open "$NAME" --screen "$SCREEN" --arg screen="$SCREEN"
        #eww open-many deactivation-area:deactivation-area-0 deactivation-area:deactivation-area-1 --arg deactivation-area-0:screen=0 --arg deactivation-area-1:screen=1
        eww update reveal_"${NAME//-/_}"=true 2>/dev/null
        send_fifo_signal open "$NAME"
    fi
fi

