#!/bin/sh
# Opens the ball on the second screen, killing whichever one was there.
#
# This has to be used —and not just saving the file— when pleamar's language
# changes: a live process keeps the compiler it was born with, so a scene that
# uses something new does not compile for it and it keeps the last good one,
# saying so in a band at the top.
P="$HOME/Proyectos/pleamar/target/release/pleamar"
cd "$(dirname "$0")" || exit 1
[ -x "$P" ] || { echo "not built: cargo build --release in ~/Proyectos/pleamar"; exit 1; }
"$P" --check marea.plm || exit 1
pkill -x pleamar
sleep 0.5
setsid nohup "$P" --scene marea.plm --screen "${1:-HDMI-A-1}" --no-hud --stall 0 > /tmp/marea-plm.log 2>&1 &
sleep 1.5
echo "open. the log is in /tmp/marea-plm.log · right-click OUTSIDE her to close it (on top of her it opens her menu)"
