#!/usr/bin/env bash

IMAGE="$1"
COLOR="$2"


awww img "$IMAGE" --namespace backdrop --transition-duration 2 --transition-type wipe --transition-angle 208 --transition-fps 144

matugen image "$IMAGE" --fallback-color "$COLOR" --prefer closest-to-fallback
eww reload --onlycss