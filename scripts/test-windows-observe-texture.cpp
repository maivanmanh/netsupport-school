// Exercise producer contention and raster leases without an actual display.
#include "observe_texture.h"
#include <cassert>
#include <cstdio>
#include <cstring>

int main() {
  char temp[MAX_PATH], path[MAX_PATH];
  GetTempPathA(MAX_PATH, temp);
  GetTempFileNameA(temp, "clt", 0, path);
  HANDLE file =
      CreateFileA(path, GENERIC_READ | GENERIC_WRITE,
                  FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  assert(file != INVALID_HANDLE_VALUE);
  LARGE_INTEGER size{};
  size.QuadPart = 32 + 1920 * 1080 * 4;
  assert(SetFilePointerEx(file, size, nullptr, FILE_BEGIN) &&
         SetEndOfFile(file));
  auto write = [&](uint64_t sequence, uint8_t blue) {
    uint8_t data[48]{};
    uint32_t dimension = 2;
    std::memcpy(data, &dimension, 4);
    std::memcpy(data + 4, &dimension, 4);
    std::memcpy(data + 8, &sequence, 8);
    for (size_t i = 32; i < sizeof(data); i += 4) {
      data[i] = blue;
      data[i + 1] = 42;
      data[i + 2] = 91;
      data[i + 3] = 255;
    }
    LARGE_INTEGER start{};
    DWORD written = 0;
    assert(SetFilePointerEx(file, start, nullptr, FILE_BEGIN));
    assert(WriteFile(file, data, sizeof(data), &written, nullptr) &&
           written == sizeof(data));
  };
  auto release = [](const FlutterDesktopPixelBuffer *frame) {
    frame->release_callback(frame->release_context);
  };
  {
    ObserveTexture texture;
    write(1, 7);
    assert(texture.Bind(path));
    const auto *first = texture.Copy();
    assert(first && first->buffer[0] == 91 && first->buffer[2] == 7);
    OVERLAPPED lock{};
    assert(LockFileEx(file, LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
                      0, MAXDWORD, MAXDWORD, &lock));
    const auto *cached = texture.Copy();
    assert(cached == first);
    assert(cached->buffer[2] == 7);
    release(cached);
    assert(UnlockFileEx(file, 0, MAXDWORD, MAXDWORD, &lock));
    write(2, 8);
    const auto *second = texture.Copy();
    assert(second && second != first);
    assert(first->buffer[2] == 7 && second->buffer[2] == 8);
    write(3, 9);
    const auto *third = texture.Copy();
    assert(third && third != second);
    write(4, 10);
    const auto *full = texture.Copy();
    assert(full == third && full->buffer[2] == 9);
    release(full);
    release(first);
    release(second);
    release(third);
    const auto *recovered = texture.Copy();
    assert(recovered && recovered->buffer[2] == 10);
    release(recovered);
    texture.Pause();
    assert(texture.Copy() == nullptr);
  }
  CloseHandle(file);
  DeleteFileA(path);
  puts("PASS: producer-lock contention retains image; leased pixels remain "
       "immutable; bounded pool recovers; pause clears");
  return 0;
}
