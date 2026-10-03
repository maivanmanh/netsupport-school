#include "flutter_window.h"

#include <shlobj.h>
#include <windows.h>

#include <algorithm>
#include <cstdint>
#include <filesystem>
#include <iomanip>
#include <optional>
#include <sstream>
#include <string>
#include <utility>
#include <variant>
#include <vector>

#include "flutter/generated_plugin_registrant.h"

namespace {

void SaveScreenshot(
    const std::string &student, const std::vector<uint8_t> &png,
    std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result) {
  if (png.empty()) {
    result->Error("empty_screenshot", "Screenshot image data is empty.");
    return;
  }

  PWSTR desktop = nullptr;
  const HRESULT folder_result = SHGetKnownFolderPath(
      FOLDERID_Desktop, KF_FLAG_DEFAULT, nullptr, &desktop);
  if (FAILED(folder_result) || desktop == nullptr) {
    result->Error("desktop_path",
                  "Could not locate the Windows Desktop folder.");
    return;
  }
  const std::filesystem::path folder(desktop);
  CoTaskMemFree(desktop);

  SYSTEMTIME now{};
  GetLocalTime(&now);
  std::wstring safe_student;
  safe_student.reserve(student.size());
  for (const char raw_ch : student) {
    const auto ch = static_cast<unsigned char>(raw_ch);
    safe_student.push_back((ch >= 'a' && ch <= 'z') ||
                                   (ch >= 'A' && ch <= 'Z') ||
                                   (ch >= '0' && ch <= '9')
                               ? static_cast<wchar_t>(ch)
                               : L'_');
  }
  if (safe_student.empty())
    safe_student = L"Student";
  std::wostringstream stem;
  stem << L"Classroom_" << safe_student << L"_" << std::setfill(L'0')
       << std::setw(4) << now.wYear << L"-" << std::setw(2) << now.wMonth
       << L"-" << std::setw(2) << now.wDay << L"_" << std::setw(2) << now.wHour
       << L"-" << std::setw(2) << now.wMinute << L"-" << std::setw(2)
       << now.wSecond << L"-" << std::setw(3) << now.wMilliseconds;

  for (unsigned int suffix = 0; suffix < 10000; ++suffix) {
    std::wstring filename = stem.str();
    if (suffix != 0)
      filename += L"_" + std::to_wstring(suffix);
    filename += L".png";
    const auto path = folder / filename;
    HANDLE file = CreateFileW(path.c_str(), GENERIC_WRITE, 0, nullptr,
                              CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) {
      const DWORD error = GetLastError();
      if (error == ERROR_FILE_EXISTS || error == ERROR_ALREADY_EXISTS)
        continue;
      result->Error("screenshot_create",
                    "Could not create screenshot on Desktop.");
      return;
    }

    size_t offset = 0;
    bool success = true;
    while (offset < png.size()) {
      const DWORD amount =
          static_cast<DWORD>(std::min<size_t>(png.size() - offset, MAXDWORD));
      DWORD written = 0;
      if (!WriteFile(file, png.data() + offset, amount, &written, nullptr) ||
          written == 0) {
        success = false;
        break;
      }
      offset += written;
    }
    if (success)
      success = FlushFileBuffers(file) != 0;
    CloseHandle(file);
    if (!success) {
      DeleteFileW(path.c_str());
      result->Error("screenshot_write",
                    "Could not write screenshot to Desktop.");
      return;
    }
    result->Success(
        flutter::EncodableValue(std::string(filename.begin(), filename.end())));
    return;
  }
  result->Error("screenshot_name",
                "Could not choose a unique screenshot filename.");
}

} // namespace

FlutterWindow::FlutterWindow(const flutter::DartProject &project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  files_channel_ =
      std::make_unique<flutter::MethodChannel<flutter::EncodableValue>>(
          flutter_controller_->engine()->messenger(), "classroom/files",
          &flutter::StandardMethodCodec::GetInstance());
  files_channel_->SetMethodCallHandler(
      [](const auto &call,
         std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>>
             result) {
        if (call.method_name() != "saveScreenshot") {
          result->NotImplemented();
          return;
        }
        const auto *args = std::get_if<flutter::EncodableMap>(call.arguments());
        if (!args) {
          result->Error("invalid_arguments",
                        "Screenshot arguments are missing.");
          return;
        }
        const auto name = args->find(flutter::EncodableValue("name"));
        const auto bytes = args->find(flutter::EncodableValue("png"));
        if (name == args->end() || bytes == args->end()) {
          result->Error("invalid_arguments",
                        "Screenshot name or image is missing.");
          return;
        }
        const auto *student = std::get_if<std::string>(&name->second);
        const auto *png = std::get_if<std::vector<uint8_t>>(&bytes->second);
        if (!student || !png) {
          result->Error("invalid_arguments",
                        "Screenshot data has an invalid format.");
          return;
        }
        SaveScreenshot(*student, *png, std::move(result));
      });
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  flutter_controller_->engine()->SetNextFrameCallback([&]() { this->Show(); });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
  case WM_FONTCHANGE:
    flutter_controller_->engine()->ReloadSystemFonts();
    break;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}
