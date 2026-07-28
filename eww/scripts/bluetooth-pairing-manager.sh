#!/usr/bin/env bash

ADDRESS="$1"
INPUT="$2"

if [[ "${#INPUT}" == "6" ]];then 
    echo answer "$ADDRESS" "$INPUT" > /tmp/eww-bluetooth-menu.fifo
fi