#!/usr/bin/env bash

pkill -f "eww/scripts" 2>/dev/null
killall eww 2>/dev/null

eww open-many bar:bar-0 bar:bar-1 --arg bar-0:screen=0 --arg bar-1:screen=1

sleep 0.5
echo hard > /tmp/eww-media-menu.fifo
echo soft > /tmp/eww-media-menu.fifo