#!/usr/bin/env bash
ACTION="$1"; shift

case "$ACTION" in
    volume)
        VALUE="$1"
        PLAYER="${2,,}"
        shift 2
        for id in $@; do wpctl set-volume "$id" "${VALUE}%"; done
        if [[ "$PLAYER" != "" ]] && playerctl --list-all 2>/dev/null | grep -qix "$PLAYER"; then
            playerctl --player "$PLAYER" volume "$(awk -v v="$VALUE" 'BEGIN{print v/100}')"
        fi
    ;;
    mute)
        STATE="$1"
        shift
        for id in $@; do wpctl set-mute "$id" "$STATE"; done
    ;;
esac
