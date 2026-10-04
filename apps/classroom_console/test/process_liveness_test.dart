import 'dart:io';

import 'package:classroom_console/process_liveness.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('unknown endpoint owner is kept', () {
    expect(windowsProcessIsAlive(null), isNull);
    expect(windowsProcessIsAlive(-1), isNull);
    expect(windowsProcessIsAlive(0x100000000), isNull);
  });

  test('running process is alive', () {
    expect(windowsProcessIsAlive(pid), Platform.isWindows ? isTrue : isNull);
  });

  test(
    'exited helper is detected even while its process handle remains',
    () async {
      if (!Platform.isWindows) return;
      final child = await Process.start('cmd.exe', ['/c', 'exit', '7']);
      expect(await child.exitCode, 7);
      expect(windowsProcessIsAlive(child.pid), isFalse);
    },
  );
}
