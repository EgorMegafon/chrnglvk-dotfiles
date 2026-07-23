#!/usr/bin/env bash

IMAGE="$1"

awww img "$IMAGE" --namespace backdrop --transition-duration 2 --transition-type wipe --transition-angle 90 --transition-fps 144

sleep 0.5

matugen image "$IMAGE" --prefer saturation