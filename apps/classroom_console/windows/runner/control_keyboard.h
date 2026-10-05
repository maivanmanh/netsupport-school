#ifndef RUNNER_CONTROL_KEYBOARD_H_
#define RUNNER_CONTROL_KEYBOARD_H_
#include <array>
#include <string>
#include <windows.h>

// One Observe window per process. Hook callback runs on the installing UI
// thread; only Win/chord events are intercepted, and only for the foreground
// Control view.
class ControlKeyboard {
public:
  static constexpr UINT kMessage = WM_APP + 0x434;
  explicit ControlKeyboard(HWND window) : window_(window) {}
  ~ControlKeyboard() { SetEnabled(false); }
  bool SetEnabled(bool enabled);
  static std::string Label(DWORD key);

private:
  static LRESULT CALLBACK Hook(int code, WPARAM message, LPARAM data);
  static thread_local ControlKeyboard *owner_;
  HWND window_;
  HHOOK hook_ = nullptr;
  std::array<bool, 256> captured_{};
};
#endif
