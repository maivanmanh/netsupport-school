import 'dart:io';
import 'package:flutter_test/flutter_test.dart';
import 'package:archive/archive_io.dart';
import 'package:classroom_console/file_transfer.dart';

void main() {
  test(
    'Stage files and ZIP folders, preserve contents and reject oversize without residue',
    () async {
      final dir = await Directory.systemTemp.createTemp('classroom-transfer-');
      try {
        final folder = Directory('${dir.path}/lesson')..createSync();
        Directory('${folder.path}/empty').createSync();
        final file = File('${folder.path}/text.txt')
          ..writeAsStringSync('Classroom content');
        final outbox = '${dir.path}/outbox';
        final staged = await stageTransfer(file.path, outbox, 1000);
        expect(File(staged['path']).readAsStringSync(), 'Classroom content');
        final zip = await stageTransfer(folder.path, outbox, 10000);
        expect(zip['name'], 'lesson.zip');
        final archive = ZipDecoder().decodeBytes(
          File(zip['path']).readAsBytesSync(),
        );
        expect(
          archive.find('lesson/text.txt')!.content,
          file.readAsBytesSync(),
        );
        expect(archive.find('lesson/empty/')!.isFile, false);
        final before = Directory(outbox).listSync().length;
        await expectLater(
          stageTransfer(file.path, outbox, 1),
          throwsStateError,
        );
        await expectLater(
          stageTransfer(folder.path, outbox, 1),
          throwsStateError,
        );
        expect(Directory(outbox).listSync().length, before);
      } finally {
        await dir.delete(recursive: true);
      }
    },
  );
}
