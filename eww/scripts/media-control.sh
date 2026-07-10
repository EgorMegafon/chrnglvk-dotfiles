#!/usr/bin/env bash

ACTION="$1"
INPUT="$2"
PLAYER="$3"
FORCE_PLAYER="$4"

case "$ACTION" in
    volume)
        change_input=""
        case "$INPUT" in
            up) change_input="+" ;;
            down) change_input="-" ;;
            *) change_input=$(awk -v input=$INPUT 'BEGIN {print input / 100}') ;;
        esac

        if [[ "$FORCE_PLAYER" == "" ]]; then
            if [[ "$PLAYER" == "spotify" ]]; then
                playerctl --player spotify volume 0.05"$change_input"
            else
                wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%"$change_input"
            fi
        else
            playerctl --player ${PLAYER} volume ${change_input}
        fi
    ;;
    seek)
        playerctl --player ${PLAYER} position ${INPUT}
    ;;
esac

echo soft > /tmp/eww-media-menu.fifo