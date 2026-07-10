#!/usr/bin/env bash

TYPE="$1"

sign="$(eww get ${TYPE}_hover_sign)"
echo soft > /tmp/eww-media.fifo;
sleep 0.5;
if [[ "$(eww get ${TYPE}_hover_sign)" = "$sign" ]]; then 
    eww update player_changing_${TYPE}=''
fi
                                                        