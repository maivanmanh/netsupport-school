import 'dart:io';
import 'package:classroom_console/agent.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'standalone Mac Teacher reuses legacy profile without copying or deleting data',
    () async {
      final home = await Directory.systemTemp.createTemp('classroom-data-test');
      addTearDown(() => home.delete(recursive: true));
      final fresh = Directory('${home.path}/Library/Application Support/new');
      final legacy = Directory(
        '${home.path}/Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole',
      );
      expect(
        (await existingMacConsoleDirectory(fresh, home.path)).path,
        fresh.path,
      );
      await legacy.create(recursive: true);
      final profile = File('${legacy.path}/teacher-profile.toml');
      await profile.writeAsString('fixture-profile');
      expect(
        (await existingMacConsoleDirectory(fresh, home.path)).path,
        legacy.path,
      );
      expect(
        (await existingMacConsoleDirectory(
          fresh,
          '${home.path}/Library/Containers/com.classroom.classroomConsole/Data',
        )).path,
        legacy.path,
      );
      expect(await profile.readAsString(), 'fixture-profile');
      expect(await fresh.exists(), false);
    },
  );
}
