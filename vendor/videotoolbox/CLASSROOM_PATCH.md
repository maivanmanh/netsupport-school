# Local patch of videotoolbox 0.20.0

Original crates.io source and licenses retained. CompressionSessionBuilder adds
with_encoder_specification(CFDictionary), retaining it until synchronous
VTCompressionSessionCreate. No callback/encode/decode behavior is changed.
Needed to request Apple's EnableLowLatencyRateControl at session creation;
RealTime and AllowFrameReordering alone do not select this encoding mode.
https://developer.apple.com/videos/play/wwdc2021/10158/
Remove this patch when upstream exposes an equivalent builder option.
