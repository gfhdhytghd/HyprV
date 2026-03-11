#!/usr/bin/env bash

# Written by Jeffrey Bouter on 2025-04-24
# Licensed under the AGPL

usage() {
  echo "$0 (light|dark)"
  exit 1
}

case $1 in
  light) THEME=light ;;
  dark) THEME=dark ;;
  *) usage;;
esac

# Set the new default theme
kwriteconfig6 --file ~/.config/konsolerc --group "Desktop Entry" --key "DefaultProfile" "${THEME}.profile"

for SERVICE in $(qdbus6 | awk '/org.kde.konsole/ { print $1 }'); do
    SESSIONS=$(qdbus6 "$SERVICE" | awk '/Sessions\// { print $1 }')
    WINDOWS=$(qdbus6 "$SERVICE" | awk '/Windows\// { print $1 }')
    # Update all existing sessions
    for session in $SESSIONS; do
      qdbus6 "$SERVICE" "$session" org.kde.konsole.Session.setProfile "$THEME"
    done

    # Set the default for all currently opened windows
    # as konsole does not re-load konsolerc for opened windows
    for window in $WINDOWS; do
        qdbus6 "$SERVICE" "$window" org.kde.konsole.Window.setDefaultProfile "$THEME"
    done
done
