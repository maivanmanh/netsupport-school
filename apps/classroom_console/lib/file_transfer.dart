import 'dart:io';
import 'dart:isolate';
import 'package:desktop_drop/desktop_drop.dart';
import 'package:archive/archive_io.dart';
import 'package:path/path.dart' as p;

/// Stage outside the UI isolate. Stream files, never read an entire payload
/// into Dart memory. The Rust Hub owns successfully submitted outbox files.
Future<Map<String, dynamic>> stageTransfer(
  String source,
  String outbox,
  int limit,
) => Isolate.run(() async {
  final root = Directory(outbox)..createSync(recursive: true);
  final staged = File(
    p.join(
      root.path,
      '${DateTime.now().microsecondsSinceEpoch}-${Isolate.current.hashCode}',
    ),
  );
  final type = FileSystemEntity.typeSync(source, followLinks: false);
  var name = p.basename(p.normalize(source));
  try {
    if (type == FileSystemEntityType.file) {
      if (File(source).lengthSync() > limit) {
        throw StateError(
          'File exceeds the transfer limit (${limit ~/ (1024 * 1024)} MB).',
        );
      }
      final output = staged.openWrite();
      var total = 0;
      try {
        await for (final bytes in File(source).openRead()) {
          total += bytes.length;
          if (total > limit) {
            throw StateError('File grew beyond the transfer limit.');
          }
          output.add(bytes);
          await output.flush();
        }
      } finally {
        await output.close();
      }
    } else if (type == FileSystemEntityType.directory) {
      name = '$name.zip';
      final encoder = ZipFileEncoder()..create(staged.path);
      var entries = 0, inputBytes = 0;
      try {
        encoder.addArchiveFile(ArchiveFile.directory('${p.basename(source)}/'));
        await for (final entry in Directory(
          source,
        ).list(recursive: true, followLinks: false)) {
          if (++entries > 100000) {
            throw StateError('Folder has too many entries.');
          }
          final kind = FileSystemEntity.typeSync(
            entry.path,
            followLinks: false,
          );
          if (kind == FileSystemEntityType.link) {
            throw StateError('Folders containing links cannot be sent.');
          }
          final relative =
              '${p.basename(source)}/${p.relative(entry.path, from: source).replaceAll('\\', '/')}';
          if (kind == FileSystemEntityType.directory) {
            encoder.addArchiveFile(ArchiveFile.directory('$relative/'));
          } else if (kind == FileSystemEntityType.file) {
            inputBytes += File(entry.path).lengthSync();
            if (inputBytes > 4 * 1024 * 1024 * 1024) {
              throw StateError('Folder contents exceed 4 GB.');
            }
            await encoder.addFile(File(entry.path), relative);
          } else {
            throw StateError('Unsupported folder entry.');
          }
          if (staged.lengthSync() > limit) {
            throw StateError('ZIP exceeds the transfer limit.');
          }
        }
      } finally {
        await encoder.close();
      }
    } else {
      throw StateError('Drop a regular file or folder.');
    }
    final size = staged.lengthSync();
    if (size > limit) {
      throw StateError(
        'File exceeds the transfer limit (${limit ~/ (1024 * 1024)} MB).',
      );
    }
    return {'path': staged.path, 'name': name, 'size': size};
  } catch (_) {
    if (staged.existsSync()) staged.deleteSync();
    rethrow;
  }
});

/// Shared Finder/Explorer drop path for roster items and Observe windows.
Future<void> sendDroppedFiles(
  List<DropItem> items, {
  required String outbox,
  required int limit,
  required bool Function() active,
  required Future<void> Function(Map<String, dynamic>) submit,
}) async {
  if (items.length > 32) throw StateError('Drop at most 32 items at a time.');
  for (final item in items) {
    if (!active()) throw StateError('Student is no longer available.');
    final bookmark = item.extraAppleBookmark;
    var access = false;
    try {
      if (Platform.isMacOS && bookmark != null) {
        access = await DesktopDrop.instance
            .startAccessingSecurityScopedResource(bookmark: bookmark);
      }
      final staged = await stageTransfer(item.path, outbox, limit);
      if (!active()) {
        await File(staged['path'] as String).delete();
        throw StateError('Student is no longer available.');
      }
      await submit(staged);
    } finally {
      if (access && bookmark != null) {
        await DesktopDrop.instance.stopAccessingSecurityScopedResource(
          bookmark: bookmark,
        );
      }
    }
  }
}
