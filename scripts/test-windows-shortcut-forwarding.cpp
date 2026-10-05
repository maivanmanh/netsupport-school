// Companion to the native hook test: focus the real Control window, inject
// Win+R, then confirm the Teacher shell did not open Run. Inspect Student Run
// independently to verify end-to-end delivery. Run only on the test desktop.
#include <fstream>
#include <string>
#include <windows.h>
static HWND control = nullptr;
static int runs = 0;
static BOOL CALLBACK Find(HWND w, LPARAM) {
  wchar_t title[256]{}, cls[80]{};
  GetWindowTextW(w, title, 256);
  GetClassNameW(w, cls, 80);
  if (IsWindowVisible(w) && std::wstring(title).rfind(L"Control", 0) == 0)
    control = w;
  if (IsWindowVisible(w) && std::wstring(title) == L"Run" &&
      std::wstring(cls) == L"#32770")
    ++runs;
  return TRUE;
}
int main(int argc, char **argv) {
  std::ofstream out(argc > 1 ? argv[1] : "shortcut-forwarding.txt");
  EnumWindows(Find, 0);
  if (!control || runs) {
    out << "FAIL expected Control window and no existing local Run\n";
    return 1;
  }
  SetForegroundWindow(control);
  Sleep(600);
  if (GetForegroundWindow() != control) {
    out << "FAIL could not focus Control\n";
    return 2;
  }
  // Focus video rather than a toolbar so Flutter delivers normal key events
  // too.
  RECT r{};
  GetWindowRect(control, &r);
  SetCursorPos((r.left + r.right) / 2, (r.top + r.bottom) / 2);
  INPUT click[2]{};
  click[0].type = INPUT_MOUSE;
  click[0].mi.dwFlags = MOUSEEVENTF_LEFTDOWN;
  click[1].type = INPUT_MOUSE;
  click[1].mi.dwFlags = MOUSEEVENTF_LEFTUP;
  if (SendInput(2, click, sizeof(INPUT)) != 2)
    return 3;
  Sleep(500);
  const WORD keys[] = {VK_LWIN, 'R', 'R', VK_LWIN};
  for (int i = 0; i < 4; ++i) {
    INPUT e{};
    e.type = INPUT_KEYBOARD;
    e.ki.wVk = keys[i];
    e.ki.dwFlags = i < 2 ? 0 : KEYEVENTF_KEYUP;
    if (SendInput(1, &e, sizeof(e)) != 1)
      return 4;
    Sleep(150);
  }
  Sleep(1500);
  runs = 0;
  EnumWindows(Find, 0);
  out << (runs ? "FAIL Teacher opened local Run"
               : "PASS Teacher did not open local Run")
      << "\n";
  return runs ? 5 : 0;
}
