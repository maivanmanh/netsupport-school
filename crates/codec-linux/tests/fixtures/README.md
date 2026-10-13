These tiny non-B-frame baseline H.264 fixtures were generated locally with
FFmpeg/libx264 from synthetic lavfi sources; they contain no user media.

```
ffmpeg -f lavfi -i testsrc2=size=64x48:rate=30 -frames:v 3 \
  -c:v libx264 -profile:v baseline -pix_fmt yuv420p -tune zerolatency \
  -x264-params keyint=30:min-keyint=30:scenecut=0:aud=1:bframes=0 \
  -f h264 sequence.h264
ffmpeg -f lavfi -i color=c=blue:size=128x72:rate=30 -frames:v 1 \
  -c:v libx264 -profile:v baseline -pix_fmt yuv420p -tune zerolatency \
  -x264-params aud=1:bframes=0 -f h264 blue-128x72.h264
ffmpeg -f lavfi -i color=c=black:size=1922x16:rate=30 -frames:v 1 \
  -c:v libx264 -profile:v baseline -pix_fmt yuv420p -tune zerolatency \
  -x264-params aud=1:bframes=0 -f h264 oversized-1922x16.h264
```

`sequence.h264` was split at each `00 00 00 01 09` access-unit delimiter into
`testsrc-64x48-{0,1,2}.h264`. Frame 0 includes SPS/PPS and IDR; 1 and 2 are
predicted frames, exercising reference continuity and immediate output.

Tests use the bundled software decoder on any supported host. They establish
codec correctness, not real Ubuntu display/LAN performance acceptance.
