import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

/// Returns null when ownership cannot be checked, so startup stays conservative.
bool? windowsProcessIsAlive(int? processId) {
  if (!Platform.isWindows ||
      processId == null ||
      processId <= 0 ||
      processId > 0xffffffff) {
    return null;
  }
  final kernel = DynamicLibrary.open('kernel32.dll');
  final openProcess = kernel
      .lookupFunction<
        Pointer<Void> Function(Uint32, Int32, Uint32),
        Pointer<Void> Function(int, int, int)
      >('OpenProcess');
  final lastError = kernel.lookupFunction<Uint32 Function(), int Function()>(
    'GetLastError',
  );
  final exitCode = kernel
      .lookupFunction<
        Int32 Function(Pointer<Void>, Pointer<Uint32>),
        int Function(Pointer<Void>, Pointer<Uint32>)
      >('GetExitCodeProcess');
  final closeHandle = kernel
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('CloseHandle');
  // Query-only access; this never terminates or changes the owner process.
  final handle = openProcess(0x1000, 0, processId);
  if (handle == nullptr) {
    // ERROR_INVALID_PARAMETER means this PID no longer exists. Access denied
    // and other errors do not establish that an owner has exited.
    return lastError() == 87 ? false : null;
  }
  final code = calloc<Uint32>();
  try {
    if (exitCode(handle, code) == 0) return null;
    return code.value == 259; // STILL_ACTIVE
  } finally {
    calloc.free(code);
    closeHandle(handle);
  }
}
