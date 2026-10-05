#ifndef RUNNER_FLUTTER_WINDOW_H_
#define RUNNER_FLUTTER_WINDOW_H_

#include <flutter/dart_project.h>
#include <flutter/flutter_view_controller.h>

#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>
#include <memory>

#include "file_picker.h"
#include "control_keyboard.h"
#include "observe_texture.h"
#include "win32_window.h"

// A window that does nothing but host a Flutter view.
class FlutterWindow : public Win32Window {
public:
  // Creates a new FlutterWindow hosting a Flutter view running |project|.
  explicit FlutterWindow(const flutter::DartProject &project);
  virtual ~FlutterWindow();

protected:
  // Win32Window:
  bool OnCreate() override;
  void OnDestroy() override;
  LRESULT MessageHandler(HWND window, UINT const message, WPARAM const wparam,
                         LPARAM const lparam) noexcept override;

private:
  std::unique_ptr<FilePicker> file_picker_;
  std::unique_ptr<ControlKeyboard> control_keyboard_;
  // The project to run.
  flutter::DartProject project_;

  // The Flutter instance hosted by this window.
  std::unique_ptr<flutter::FlutterViewController> flutter_controller_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      files_channel_;
  std::unique_ptr<flutter::MethodChannel<flutter::EncodableValue>>
      observe_channel_;
  std::shared_ptr<ObserveTexture> observe_texture_;
  std::unique_ptr<flutter::TextureVariant> texture_variant_;
  flutter::TextureRegistrar *textures_ = nullptr;
  int64_t texture_id_ = -1;
  UINT_PTR texture_timer_ = 0;
};

#endif // RUNNER_FLUTTER_WINDOW_H_
