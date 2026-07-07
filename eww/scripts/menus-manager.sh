#!/usr/bin/env bash

ACTION="$1"
NAME="$2"
SCREEN="$3"

if [[ "$ACTION" == "close-menus" ]]; then 
    eww active-windows | while IFS=':' read -r id name; do
        id=$(echo "$id" | xargs)
        name=$(echo "$name" | xargs)
        if [[ "$name" != "bar" ]]; then
            eww close "$id"
        fi
    done

    pkill -USR2 -f "target/release/media-menu"
fi

if [[ "$ACTION" == "open-menu" ]]; then 
    if eww active-windows | grep -q "^.*: ${NAME}$"; then
        eww close "$NAME"; eww close deactivation-area-0 deactivation-area-1
        pkill -USR2 -f "target/release/${NAME}"
    else
        eww open "$NAME" --screen "$SCREEN" --arg screen="$SCREEN"
        eww open-many deactivation-area:deactivation-area-0 deactivation-area:deactivation-area-1 --arg deactivation-area-0:screen=0 --arg deactivation-area-1:screen=1
        pkill -USR1 -f "target/release/${NAME}"
    fi
fi