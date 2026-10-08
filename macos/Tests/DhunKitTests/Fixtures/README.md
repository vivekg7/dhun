# Test fixtures

`sweep-1.webm` and `sweep-2.webm` are one second of a rising tone, mono at
48 kHz, cut at sample 24003 and each half encoded to Opus in WebM, as
YouTube downloaders save `.opus` files. Made with FFmpeg 9:

```sh
ffmpeg -f lavfi -i "aevalsrc=0.4*sin(2*PI*(200+300*t)*t):s=48000:d=1" -ac 1 sweep.wav
ffmpeg -i sweep.wav -af atrim=end_sample=24003 h1.wav
ffmpeg -i sweep.wav -af "atrim=start_sample=24003,asetpts=PTS-STARTPTS" h2.wav
ffmpeg -i h1.wav -c:a libopus -b:a 64k -f webm sweep-1.webm   # and h2 → sweep-2
```
