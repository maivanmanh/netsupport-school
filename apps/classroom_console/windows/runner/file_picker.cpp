#include "file_picker.h"
#include <commdlg.h>
#include <filesystem>
#include <objbase.h>
#include <stdexcept>
#include <vector>

namespace {
constexpr UINT kCompletion = WM_APP + 0x43;
constexpr uint64_t kMaxFile = uint64_t{4} * 1024 * 1024 * 1024;
using Result = flutter::MethodResult<flutter::EncodableValue>;
struct Completion {
  std::unique_ptr<Result> result;
  flutter::EncodableMap value;
  std::string error;
};
std::string Utf8(const std::wstring &value) {
  int size = WideCharToMultiByte(CP_UTF8, 0, value.data(),
                                 static_cast<int>(value.size()), nullptr, 0,
                                 nullptr, nullptr);
  std::string text(static_cast<size_t>(size), '\0');
  WideCharToMultiByte(CP_UTF8, 0, value.data(), static_cast<int>(value.size()),
                      text.data(), size, nullptr, nullptr);
  return text;
}
DWORD CALLBACK Progress(LARGE_INTEGER total, LARGE_INTEGER, LARGE_INTEGER,
                        LARGE_INTEGER, DWORD, DWORD, HANDLE, HANDLE,
                        LPVOID context) {
  auto *stopping = static_cast<std::atomic<bool> *>(context);
  return stopping->load() || static_cast<uint64_t>(total.QuadPart) > kMaxFile
             ? PROGRESS_CANCEL
             : PROGRESS_CONTINUE;
}
} // namespace

FilePicker::~FilePicker() {
  stopping_->store(true);
  if (worker_.joinable())
    worker_.join();
  MSG message{};
  while (PeekMessageW(&message, owner_, kCompletion, kCompletion, PM_REMOVE))
    delete reinterpret_cast<Completion *>(message.lParam);
}

void FilePicker::Begin(const std::string &directory,
                       std::unique_ptr<Result> result) {
  if (busy_) {
    result->Error("file_busy", "A file is already being prepared.");
    return;
  }
  std::vector<wchar_t> filename(32768, L'\0');
  OPENFILENAMEW dialog{};
  dialog.lStructSize = sizeof(dialog);
  dialog.hwndOwner = owner_;
  dialog.lpstrTitle = L"Send a file to Students";
  dialog.lpstrFilter = L"All files\0*.*\0\0";
  dialog.lpstrFile = filename.data();
  dialog.nMaxFile = static_cast<DWORD>(filename.size());
  dialog.Flags =
      OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
  if (!GetOpenFileNameW(&dialog)) {
    if (CommDlgExtendedError())
      result->Error("file_picker", "Could not open the Windows file picker.");
    else
      result->Success();
    return;
  }
  if (worker_.joinable())
    worker_.join();
  busy_ = true;
  const auto owner = owner_;
  const auto stopping = stopping_;
  const std::filesystem::path source(filename.data());
  worker_ = std::thread([owner, stopping, source, directory,
                         result = std::move(result)]() mutable {
    auto completion = std::make_unique<Completion>();
    completion->result = std::move(result);
    std::filesystem::path staged;
    try {
      if (!std::filesystem::is_regular_file(source) ||
          std::filesystem::file_size(source) > kMaxFile)
        throw std::runtime_error("Choose a regular file up to 4 GiB.");
      auto outbox = std::filesystem::u8path(directory) / L"outbox";
      std::filesystem::create_directories(outbox);
      GUID id{};
      wchar_t name[40]{};
      if (FAILED(CoCreateGuid(&id)) || !StringFromGUID2(id, name, 40))
        throw std::runtime_error("Could not prepare an outgoing filename.");
      staged = outbox / name;
      if (!CopyFileExW(source.c_str(), staged.c_str(), Progress, stopping.get(),
                       nullptr, COPY_FILE_FAIL_IF_EXISTS))
        throw std::runtime_error("Could not copy the selected file. Check "
                                 "access and available disk space.");
      const auto size = std::filesystem::file_size(staged);
      if (size > kMaxFile)
        throw std::runtime_error("File exceeds 4 GiB.");
      completion->value = {
          {flutter::EncodableValue("path"),
           flutter::EncodableValue(Utf8(staged.wstring()))},
          {flutter::EncodableValue("name"),
           flutter::EncodableValue(Utf8(source.filename().wstring()))},
          {flutter::EncodableValue("size"),
           flutter::EncodableValue(static_cast<int64_t>(size))},
      };
    } catch (const std::exception &error) {
      completion->error = error.what();
    }
    if (!completion->error.empty() || stopping->load()) {
      std::error_code ignored;
      if (!staged.empty())
        std::filesystem::remove(staged, ignored);
    }
    if (!stopping->load() &&
        PostMessageW(owner, kCompletion, 0,
                     reinterpret_cast<LPARAM>(completion.get())))
      completion.release();
  });
}

bool FilePicker::Handle(UINT message, LPARAM parameter) {
  if (message != kCompletion)
    return false;
  auto completion =
      std::unique_ptr<Completion>(reinterpret_cast<Completion *>(parameter));
  if (worker_.joinable())
    worker_.join();
  busy_ = false;
  if (completion->error.empty())
    completion->result->Success(flutter::EncodableValue(completion->value));
  else
    completion->result->Error("file_prepare", completion->error);
  return true;
}
