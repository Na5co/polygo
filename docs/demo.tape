# Recorded by scripts/demo.sh — do not run directly (vhs 0.12 + ffmpeg 9 cannot
# encode the GIF itself; the script assembles docs/demo.gif from these frames).
Output "docs/demo-frames/"
Set Shell "zsh"
Set FontSize 15
Set Width 1000
Set Height 470
Set Framerate 12
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms
Set Padding 18
Hide
Type "cd $POLYGO_DEMO_DIR && clear"
Enter
Show
Type "polygo init"
Enter
Wait+Screen@10s /next:/
Sleep 1.2s
Type "polygo doctor"
Enter
Wait+Screen@10s /ready/
Sleep 1.5s
Type "polygo translate -v"
Enter
Wait+Screen@120s /translated \d+ string/
Sleep 2s
Type "polygo check"
Enter
Wait+Screen@10s /check: ok/
Sleep 3s
