#ifndef CLASSROOM_FILE_PICKER_H_
#define CLASSROOM_FILE_PICKER_H_
#include <atomic>
#include <flutter/method_result.h>
#include <flutter/standard_method_codec.h>
#include <memory>
#include <string>
#include <thread>
#include <windows.h>

class FilePicker {
public:
  explicit FilePicker(HWND owner) : owner_(owner) {}
  ~FilePicker();
  void
  Begin(const std::string &directory,
        std::unique_ptr<flutter::MethodResult<flutter::EncodableValue>> result);
  bool Handle(UINT message, LPARAM parameter);

private:
  HWND owner_;
  bool busy_ = false;
  std::shared_ptr<std::atomic<bool>> stopping_ =
      std::make_shared<std::atomic<bool>>(false);
  std::thread worker_;
};
#endif
