#include "control_keyboard.h"

thread_local ControlKeyboard *ControlKeyboard::owner_ = nullptr;

bool ControlKeyboard::SetEnabled(bool enabled) {
  if (!enabled) {
    if (hook_)
      UnhookWindowsHookEx(hook_);
    hook_ = nullptr;
    if (owner_ == this)
      owner_ = nullptr;
    captured_.fill(false);
    return true;
  }
  if (hook_)
    return true;
  if (owner_ && owner_ != this)
    return false;
  owner_ = this;
  hook_ = SetWindowsHookExW(WH_KEYBOARD_LL, Hook, GetModuleHandleW(nullptr), 0);
  if (!hook_)
    owner_ = nullptr;
  return hook_ != nullptr;
}

std::string ControlKeyboard::Label(DWORD key) {
  if (key >= 'A' && key <= 'Z')
    return std::string(1, static_cast<char>(key + 'a' - 'A'));
  if (key >= '0' && key <= '9')
    return std::string(1, static_cast<char>(key));
  switch (key) {
  case VK_LWIN:
  case VK_RWIN:
    return "Meta";
  case VK_RETURN:
    return "Enter";
  case VK_SPACE:
    return " ";
  case VK_TAB:
    return "Tab";
  case VK_ESCAPE:
    return "Escape";
  case VK_BACK:
    return "Backspace";
  case VK_DELETE:
    return "Delete";
  case VK_HOME:
    return "Home";
  case VK_END:
    return "End";
  case VK_PRIOR:
    return "PageUp";
  case VK_NEXT:
    return "PageDown";
  case VK_LEFT:
    return "ArrowLeft";
  case VK_RIGHT:
    return "ArrowRight";
  case VK_UP:
    return "ArrowUp";
  case VK_DOWN:
    return "ArrowDown";
  default:
    return {};
  }
}

LRESULT CALLBACK ControlKeyboard::Hook(int code, WPARAM message, LPARAM data) {
  auto *self = owner_;
  if (code != HC_ACTION || !self || GetForegroundWindow() != self->window_)
    return CallNextHookEx(nullptr, code, message, data);
  const auto &event = *reinterpret_cast<const KBDLLHOOKSTRUCT *>(data);
  const DWORD key = event.vkCode;
  if (key >= self->captured_.size())
    return CallNextHookEx(nullptr, code, message, data);
  const bool down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
  const bool up = message == WM_KEYUP || message == WM_SYSKEYUP;
  const bool win = key == VK_LWIN || key == VK_RWIN;
  const bool chord = self->captured_[VK_LWIN] || self->captured_[VK_RWIN];
  if ((!down && !up) ||
      (!win && !self->captured_[key] && !(chord && !Label(key).empty())))
    return CallNextHookEx(nullptr, code, message, data);
  // Never perform IPC/network work inside the low-level callback. Ordered
  // native messages relay to Dart on this UI thread. On queue failure leave
  // input local.
  if (!PostMessageW(self->window_, kMessage, key, down ? 1 : 0))
    return CallNextHookEx(nullptr, code, message, data);
  self->captured_[key] = down;
  return 1; // Windows shell must not act on the teacher's Win shortcut.
}
