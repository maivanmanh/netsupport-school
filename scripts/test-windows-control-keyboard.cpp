// Run in the active Windows desktop. Uses the real shell and same production
// hook, asserts routing and removes only Run dialogs opened by this test.
#include "../apps/classroom_console/windows/runner/control_keyboard.h"
#include <fstream>
#include <iostream>
#include <vector>
#include <windows.h>

static std::vector<std::pair<DWORD, bool>> events;
static LRESULT CALLBACK Proc(HWND w, UINT m, WPARAM a, LPARAM b) {
  if (m == ControlKeyboard::kMessage) {
    events.emplace_back(static_cast<DWORD>(a), b != 0);
    return 0;
  }
  return DefWindowProcW(w, m, a, b);
}
static void Pump(int milliseconds) {
  const auto end = GetTickCount64() + milliseconds;
  while (GetTickCount64() < end) {
    MSG m{};
    while (PeekMessageW(&m, nullptr, 0, 0, PM_REMOVE)) {
      TranslateMessage(&m);
      DispatchMessageW(&m);
    }
    Sleep(5);
  }
}
static bool WinR() {
  for (auto value : std::vector<std::pair<WORD, bool>>{
           {VK_LWIN, true}, {'R', true}, {'R', false}, {VK_LWIN, false}}) {
    INPUT i{};
    i.type = INPUT_KEYBOARD;
    i.ki.wVk = value.first;
    i.ki.dwFlags = value.second ? 0 : KEYEVENTF_KEYUP;
    if (SendInput(1, &i, sizeof(i)) != 1)
      return false;
    Pump(100);
  }
  Pump(600);
  return true;
}
static BOOL CALLBACK FindRun(HWND w, LPARAM p) {
  wchar_t name[80]{}, cls[80]{};
  GetWindowTextW(w, name, 80);
  GetClassNameW(w, cls, 80);
  if (IsWindowVisible(w) && std::wstring(cls) == L"#32770" &&
      std::wstring(name) == L"Run")
    reinterpret_cast<std::vector<HWND> *>(p)->push_back(w);
  return TRUE;
}
static std::vector<HWND> Runs() {
  std::vector<HWND> r;
  EnumWindows(FindRun, reinterpret_cast<LPARAM>(&r));
  return r;
}
int main(int argc, char **argv) {
  std::ofstream report(argc > 1 ? argv[1] : "keyboard-test.txt");
  auto check = [&](bool ok, const char *label) {
    report << (ok ? "PASS " : "FAIL ") << label << std::endl;
    return ok;
  };
  if (!check(Runs().empty(), "No pre-existing Run dialog"))
    return 1;
  WNDCLASSW cls{};
  cls.lpfnWndProc = Proc;
  cls.hInstance = GetModuleHandleW(nullptr);
  cls.lpszClassName = L"ClassroomKeyboardTest";
  RegisterClassW(&cls);
  HWND w = CreateWindowW(cls.lpszClassName, L"Classroom Keyboard Control Test",
                         WS_OVERLAPPEDWINDOW | WS_VISIBLE, 50, 50, 500, 300,
                         nullptr, nullptr, cls.hInstance, nullptr);
  SetForegroundWindow(w);
  Pump(300);
  ControlKeyboard capture(w);
  if (!check(GetForegroundWindow() == w && capture.SetEnabled(true),
             "Foreground Control hook installed"))
    return 2;
  if (!check(WinR(), "SendInput Win+R accepted"))
    return 3;
  const std::vector<std::pair<DWORD, bool>> expected{
      {VK_LWIN, true}, {'R', true}, {'R', false}, {VK_LWIN, false}};
  bool ok = check(events == expected,
                  "Exactly ordered Win/R down/up forwarded once") &
            check(Runs().empty(), "Control Win+R does not open local Run");
  capture.SetEnabled(false);
  events.clear();
  SetForegroundWindow(w);
  Pump(200);
  WinR();
  auto runs = Runs();
  ok = check(!runs.empty(), "Observe/disabled Win+R opens local Run") &
       check(events.empty(), "Disabled hook forwards nothing") & ok;
  for (HWND run : runs) {
    PostMessageW(run, WM_CLOSE, 0, 0);
  }
  Pump(300);
  capture.SetEnabled(true);
  HWND other = CreateWindowW(cls.lpszClassName, L"Other local application",
                             WS_OVERLAPPEDWINDOW | WS_VISIBLE, 100, 100, 500,
                             300, nullptr, nullptr, cls.hInstance, nullptr);
  SetForegroundWindow(other);
  Pump(300);
  WinR();
  runs = Runs();
  ok = check(!runs.empty(), "Unfocused Control leaves Win+R local") &
       check(events.empty(), "Unfocused Control forwards nothing") & ok;
  for (HWND run : runs)
    PostMessageW(run, WM_CLOSE, 0, 0);
  capture.SetEnabled(false);
  DestroyWindow(other);
  DestroyWindow(w);
  return ok ? 0 : 4;
}
