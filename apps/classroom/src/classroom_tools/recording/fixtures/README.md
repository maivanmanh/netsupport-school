# Synthetic H.264 MP4 test fixture

`blue-720p.h264` is a generated solid-blue 1280×720 frame, with no audio,
real screen pixels or personal data. Generated locally using:

```sh
ffmpeg -hide_banner -loglevel error -f lavfi -i color=c=blue:s=1280x720:r=30 \
  -frames:v 1 -c:v libx264 -profile:v main -pix_fmt yuv420p -tune zerolatency \
  -x264-params 'keyint=1:repeat-headers=1:bframes=0' -f h264 blue-720p.h264
```

The mux test reuses the IDR at 0 and 100 ms and holds the second frame to 1 s.
This checks real AVC configuration and variable sample durations without invoking
an encoder in unit tests. Setting `CLASSROOM_RECORDING_MP4_TEST_OUTPUT` retains a
copy of the test output for external ffprobe/ffmpeg inspection.
