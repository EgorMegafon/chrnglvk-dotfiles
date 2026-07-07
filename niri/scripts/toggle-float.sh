#!/usr/bin/env bash

niri msg action toggle-window-floating
IS_FLOATING=$(niri msg --json focused-window | jq '.is_floating')

if [ "$IS_FLOATING" == "true" ]; then
    niri msg action set-window-width "50%"
    niri msg action set-window-height "50%"
    sleep 0.05
    niri msg action center-window
fi
else
    niri msg action set-window-heigth "50%"
fi