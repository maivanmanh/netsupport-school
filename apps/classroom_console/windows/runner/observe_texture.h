#ifndef CLASSROOM_OBSERVE_TEXTURE_H_
#define CLASSROOM_OBSERVE_TEXTURE_H_

#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>
#include <flutter/texture_registrar.h>
#include <windows.h>

#include <array>
#include <cstdint>
#include <mutex>
#include <string>
#include <vector>

// Fixed-size latest-frame map shared with observation::Output in Rust. The
// Win32 whole-file lock matches fs2's LockFileEx and is always nonblocking.
// Flutter owns each returned RGBA buffer until its release callback. Three
// reusable slots bound memory even when the raster thread falls behind.
class ObserveTexture {
public:
  ObserveTexture();
  ~ObserveTexture();
  bool Bind(const std::string &path);
  void Pause();
  bool HasNewFrame();
  const FlutterDesktopPixelBuffer *Copy();
  flutter::EncodableMap Stats();

private:
  static constexpr size_t kCapacity = 32 + 1920 * 1080 * 4;
  struct Buffer {
    std::vector<uint8_t> pixels;
    FlutterDesktopPixelBuffer descriptor{};
    std::mutex mutex;
    size_t leases = 0;
  };
  bool Connect();
  void Close();
  bool LockMap();
  void UnlockMap();
  const FlutterDesktopPixelBuffer *CachedFrame();
  std::mutex mutex_;
  std::array<Buffer, 3> buffers_;
  Buffer *latest_ = nullptr;
  uint64_t cached_returns_ = 0;
  std::string path_, error_;
  HANDLE file_ = INVALID_HANDLE_VALUE;
  HANDLE mapping_ = nullptr;
  const uint8_t *map_ = nullptr;
  OVERLAPPED overlapped_{};
  bool active_ = false;
  uint64_t copied_ = 0, source_ = 0, copies_ = 0;
  uint64_t notifications_ = 0, attempts_ = 0, failures_ = 0, lock_misses_ = 0;
  uint32_t width_ = 0, height_ = 0;
};
#endif
