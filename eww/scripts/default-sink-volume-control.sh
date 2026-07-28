#!/usr/bin/env bash

DIRECTION="$1"

change_input=""
case "$DIRECTION" in
    up) change_input="+" ;;
    down) change_input="-" ;;
esac
wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%"$change_input"
