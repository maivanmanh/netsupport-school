#include "observe_texture.h"

#include <cstring>

ObserveTexture::ObserveTexture() = default;
ObserveTexture::~ObserveTexture() { Close(); }

void ObserveTexture::Close() {
  if (map_)
    UnmapViewOfFile(map_);
  if (mapping_)
    CloseHandle(mapping_);
  if (file_ != INVALID_HANDLE_VALUE)
    CloseHandle(file_);
  map_ = nullptr;
  mapping_ = nullptr;
  file_ = INVALID_HANDLE_VALUE;
}

bool ObserveTexture::Connect() {
  if (map_)
    return true;
  const int length = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS,
                                         path_.c_str(), -1, nullptr, 0);
  if (length <= 0) {
    error_ = "Invalid UTF-8 surface path";
    return false;
  }
  std::wstring path(static_cast<size_t>(length), L'\0');
  MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path_.c_str(), -1,
                      path.data(), length);
  file_ = CreateFileW(path.c_str(), GENERIC_READ,
                      FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                      nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file_ == INVALID_HANDLE_VALUE) {
    error_ = "Surface open error " + std::to_string(GetLastError());
    return false;
  }
  LARGE_INTEGER size{};
  if (!GetFileSizeEx(file_, &size) || size.QuadPart != kCapacity) {
    error_ = "Unexpected surface size";
    Close();
    return false;
  }
  mapping_ = CreateFileMappingW(file_, nullptr, PAGE_READONLY, 0, 0, nullptr);
  if (mapping_) {
    map_ = static_cast<const uint8_t *>(
        MapViewOfFile(mapping_, FILE_MAP_READ, 0, 0, kCapacity));
  }
  if (!map_) {
    error_ = "Surface mapping error " + std::to_string(GetLastError());
    Close();
    return false;
  }
  error_.clear();
  return true;
}

bool ObserveTexture::Bind(const std::string &path) {
  std::lock_guard<std::mutex> lock(mutex_);
  if (path != path_) {
    Close();
    path_ = path;
    copied_ = 0;
    source_ = 0;
    width_ = 0;
    height_ = 0;
  }
  active_ = true;
  // The producer can create its surface after the IPC state arrives. Retry
  // from the timer rather than making attach fail for that startup race.
  return Connect();
}

void ObserveTexture::Pause() {
  std::lock_guard<std::mutex> lock(mutex_);
  active_ = false;
}

bool ObserveTexture::LockMap() {
  if (LockFileEx(file_, LOCKFILE_FAIL_IMMEDIATELY, 0, MAXDWORD, MAXDWORD,
                 &overlapped_))
    return true;
  ++lock_misses_;
  return false;
}
void ObserveTexture::UnlockMap() {
  UnlockFileEx(file_, 0, MAXDWORD, MAXDWORD, &overlapped_);
}

bool ObserveTexture::HasNewFrame() {
  std::lock_guard<std::mutex> lock(mutex_);
  if (!active_ || !Connect() || !LockMap())
    return false;
  std::memcpy(&source_, map_ + 8, sizeof(source_));
  UnlockMap();
  if (source_ == 0 || source_ == copied_)
    return false;
  ++notifications_;
  return true;
}

const FlutterDesktopPixelBuffer *ObserveTexture::Copy() {
  std::lock_guard<std::mutex> lock(mutex_);
  ++attempts_;
  if (!active_ || !Connect() || !LockMap()) {
    ++failures_;
    return nullptr;
  }
  uint32_t width = 0, height = 0;
  uint64_t sequence = 0;
  std::memcpy(&width, map_, sizeof(width));
  std::memcpy(&height, map_ + 4, sizeof(height));
  std::memcpy(&sequence, map_ + 8, sizeof(sequence));
  if (width == 0 || height == 0 || width > 1920 || height > 1080 ||
      sequence == 0) {
    UnlockMap();
    ++failures_;
    return nullptr;
  }
  for (auto &slot : buffers_) {
    std::lock_guard<std::mutex> buffer_lock(slot.mutex);
    if (slot.busy)
      continue;
    slot.pixels.resize(static_cast<size_t>(width) * height * 4);
    // Flutter's PixelBufferTexture contract is RGBA, Rust's surface is BGRA.
    for (size_t i = 0; i < slot.pixels.size(); i += 4) {
      slot.pixels[i] = map_[32 + i + 2];
      slot.pixels[i + 1] = map_[32 + i + 1];
      slot.pixels[i + 2] = map_[32 + i];
      slot.pixels[i + 3] = map_[32 + i + 3];
    }
    UnlockMap();
    slot.busy = true;
    slot.descriptor.buffer = slot.pixels.data();
    slot.descriptor.width = width;
    slot.descriptor.height = height;
    slot.descriptor.release_context = &slot;
    slot.descriptor.release_callback = [](void *context) {
      auto *buffer = static_cast<Buffer *>(context);
      std::lock_guard<std::mutex> release_lock(buffer->mutex);
      buffer->busy = false;
    };
    if (sequence != copied_)
      ++copies_;
    copied_ = sequence;
    source_ = sequence;
    width_ = width;
    height_ = height;
    return &slot.descriptor;
  }
  UnlockMap();
  ++failures_;
  return nullptr;
}

flutter::EncodableMap ObserveTexture::Stats() {
  std::lock_guard<std::mutex> lock(mutex_);
  using V = flutter::EncodableValue;
  return {{V("width"), V(static_cast<int32_t>(width_))},
          {V("height"), V(static_cast<int32_t>(height_))},
          {V("sequence"), V(static_cast<int64_t>(copied_))},
          {V("source_sequence"), V(static_cast<int64_t>(source_))},
          {V("native_frames"), V(static_cast<int64_t>(copies_))},
          {V("frame_notifications"), V(static_cast<int64_t>(notifications_))},
          {V("copy_attempts"), V(static_cast<int64_t>(attempts_))},
          {V("copy_failures"), V(static_cast<int64_t>(failures_))},
          {V("lock_misses"), V(static_cast<int64_t>(lock_misses_))},
          {V("connected"), V(map_ != nullptr)},
          {V("active"), V(active_)},
          {V("connect_error"), V(error_)}};
}
