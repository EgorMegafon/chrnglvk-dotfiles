#!/usr/bin/env bash

ID="$1"
ACTION="$2"
INPUT="$3"

case "$ACTION" in
    volume)
        change_input=""
        case "$INPUT" in
            up) change_input="5%+" ;;
            down) change_input="5%-" ;;
            *) change_input="${INPUT}%" ;;
        esac

        echo "$change_input" > /tmp/pw-debug
        wpctl set-volume "$ID" "$change_input"
    ;;
    mute)
        wpctl set-mute "$ID" toggle
    ;;
    default)
        wpctl set-default "$ID"
    ;;
esac
