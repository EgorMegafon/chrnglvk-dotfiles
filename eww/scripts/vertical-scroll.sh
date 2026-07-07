#!/usr/bin/env bash
case "$1" in
  up)   niri msg action focus-workspace-up ;;
  down) niri msg action focus-workspace-down ;;
esac