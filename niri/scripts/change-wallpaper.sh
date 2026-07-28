#!/usr/bin/env bash

IMAGE="$1"

matugen image "$IMAGE" 

awww img "$IMAGE" --namespace backdrop --transition-duration 2 --transition-type wipe --transition-angle 208 --transition-fps 144
