#!/usr/bin/env bash

DIR="$1"
PLAYER="$2"
FORCE_PLAYER="$3"

change_symbol=""
case "$DIR" in
    up) change_symbol="+" ;;
    down) change_symbol="-" ;;
esac

if [[ "$FORCE_PLAYER" == "" ]]; then
    if [[ "$PLAYER" == "spotify" ]]; then
        playerctl --player spotify volume 0.05"$change_symbol"
    else
        wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%"$change_symbol"
    fi
else
    playerctl --player "$PLAYER" volume 0.05"$change_symbol"
fi