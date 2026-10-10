import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:window_manager/window_manager.dart';
import 'process_liveness.dart';

// The standalone Mac Teacher needs assistive system-control APIs. Reuse its
// previous sandbox container data rather than creating a second room/profile.
Future<Directory> existingMacConsoleDirectory(
  Directory suggested,
  String? home,
) async {
  if (home == null || home.isEmpty) return suggested;
  const suffix = '/Library/Containers/com.classroom.classroomConsole/Data';
  final actualHome = home.endsWith(suffix)
      ? home.substring(0, home.length - suffix.length)
      : home;
  final legacy = Directory(
    '$actualHome$suffix/Library/Application Support/com.classroom.classroomConsole',
  );
  for (final name in [
    'teacher-profile.toml',
    'teacher-settings.toml',
    'console-settings.json',
  ]) {
    if (await File('${legacy.path}/$name').exists()) return legacy;
  }
  return suggested;
}

class Agent extends ChangeNotifier {
  Agent(this.args, {this.closeApplication});
  final List<String> args;
  final Future<void> Function()? closeApplication;
  bool shutdownReceived = false;
  bool reconfiguring = false;
  Map<String, dynamic> preferences = {};
  Directory? dataDirectory;
  Map<String, dynamic> state = {};
  final Map<String, Uint8List> images = {};
  final Map<int, Completer<Map<String, dynamic>>> pending = {};
  Socket? socket;
  String? error;
  late File endpoint;
  int serial = 0;
  bool disposed = false;
  Timer? reconnect;
  Timer? thumbnailSizeTimer;
  int? requestedThumbnailWidth;
  int? sentThumbnailWidth;
  bool exiting = false;
  bool logBusy = false;
  final Map<String, Map<String, dynamic>> known = {};
  final seenOnlineIds = <String>{};
  String roomKey = '';
  Future<void> writes = Future.value();
  bool commandBusy = false;
  String? notice;
  final Map<String, Map<String, dynamic>> pendingObserveMoves = {};
  Timer? observeMoveFlush;
  int observeLaunch = 0;
  int lastObserveFocus = 0;

  /// Observe, file, activity, Internet-access and VS Code windows are
  /// dependent UIs of the main console.
  bool get dependent =>
      option('--observe-id') != null ||
      option('--files-id') != null ||
      option('--activity-id') != null ||
      option('--internet-ids') != null ||
      option('--vscode-ids') != null ||
      option('--terminal-id') != null;
  // Terminal windows share the authenticated coordinator, but stream raw bytes
  // over their own bounded attachment rather than roster snapshots.
  String? pendingObserveFocus;

  void ingestState(Map<String, dynamic> value) {
    final request = value['observe_focus'] as List?;
    if (request != null && request.length >= 3 && request[1] is num) {
      final sequence = (request[1] as num).toInt();
      if (sequence > lastObserveFocus) {
        lastObserveFocus = sequence;
        if (!dependent && pendingObserveFocus == request[0]) {
          pendingObserveFocus = null;
          // The clicked console owns foreground permission on Windows. Grant
          // it to/focus the independent UI before its own activation attempt.
          unawaited(
            const MethodChannel('classroom/observe')
                .invokeMethod<void>('activateWindow', request[2])
                .catchError((Object e) {
                  unawaited(log('Observe activation failed: $e'));
                }),
          );
        }
      }
    }
    final key = '${value['teacher_ip']}:${value['control_port']}';
    if (roomKey != key) {
      known.clear();
      seenOnlineIds.clear();
      images.clear();
      roomKey = key;
    }
    state = value;
    var dirty = false;
    final rows = (value['students'] as List? ?? [])
        .map((r) => Map<String, dynamic>.from(r as Map))
        .toList();
    final liveIds = rows
        .where((r) => r['online'] == true)
        .map((r) => r['id'])
        .toSet();
    final retired = <String>{};
    for (final live in rows.where((r) => r['online'] == true)) {
      final name = '${live['name'] ?? ''}'.trim().toLowerCase();
      final ip = '${live['ip'] ?? ''}'.trim();
      if (name.isEmpty ||
          name == 'unknown-host' ||
          ip.isEmpty ||
          name.startsWith('sim-')) {
        continue;
      }
      for (final old in [...known.values, ...rows]) {
        final oldId = old['id'] as String;
        if (oldId != live['id'] &&
            !liveIds.contains(oldId) &&
            '${old['name'] ?? ''}'.trim().toLowerCase() == name &&
            '${old['ip'] ?? ''}'.trim() == ip) {
          retired.add(oldId);
        }
      }
    }
    for (final id in retired) {
      dirty = known.remove(id) != null || dirty;
      images.remove(id);
      seenOnlineIds.remove(id);
    }
    // Only retire offline aliases; distinct concurrently connected endpoints remain.
    state = {
      ...value,
      'students': rows.where((r) => !retired.contains(r['id'])).toList(),
    };
    for (final row in (state['students'] as List? ?? [])) {
      final r = Map<String, dynamic>.from(row as Map);
      final id = r['id'] as String;
      if (r['online'] == true) {
        seenOnlineIds.add(id);
      }
      // scripts/simulate-class.py Students (sim-*) are shown while online
      // but never remembered, so test runs leave no offline roster entries.
      final simulated = '${r['name'] ?? ''}'.startsWith('sim-');
      final old = known[id];
      if (!simulated &&
          (old == null || old['name'] != r['name'] || old['ip'] != r['ip'])) {
        dirty = true;
      }
      if (!simulated) known[id] = {'id': id, 'name': r['name'], 'ip': r['ip']};
      final thumbnail = r['thumbnail'] as Map<String, dynamic>?;
      if (thumbnail != null) {
        images[id] = base64Decode(thumbnail['jpeg'] as String);
      }
    }
    while (known.length > 256) {
      known.remove(known.keys.first);
    }
    if (dirty && dataDirectory != null) {
      final data = jsonEncode({
        'room': roomKey,
        'students': known.values.toList(),
      });
      writes = writes
          .then((_) async {
            final f = File('${dataDirectory!.path}/known-students.json');
            final temp = File('${f.path}.tmp');
            await temp.writeAsString(data, flush: true);
            if (Platform.isWindows && await f.exists()) await f.delete();
            await temp.rename(f.path);
          })
          .catchError((Object e) {
            log('roster save failed: $e');
          });
    }
  }

  void thumbnailSize(double itemWidth) {
    final tier = itemWidth <= 280 ? 240 : (itemWidth <= 400 ? 360 : 480);
    if (tier == requestedThumbnailWidth) return;
    requestedThumbnailWidth = tier;
    thumbnailSizeTimer?.cancel();
    thumbnailSizeTimer = Timer(const Duration(milliseconds: 900), () async {
      if (disposed || !connected || sentThumbnailWidth == tier) return;
      try {
        await command('thumbnail_size', const [], {'width': tier});
        sentThumbnailWidth = tier;
      } catch (e) {
        await log('thumbnail tier request failed: $e');
      }
    });
  }

  Future<void> rescan() async {
    notice = 'Refreshing connections…';
    notifyListeners();
    await command('rescan');
    await Future<void>.delayed(const Duration(seconds: 3));
    notice = connected
        ? 'Connections refreshed · Students reconnect automatically'
        : null;
    if (!disposed) notifyListeners();
  }

  Future<void> log(String event) async {
    if (logBusy || dataDirectory == null) {
      return;
    }
    logBusy = true;
    try {
      await File(
        '${dataDirectory!.path}/${!dependent ? 'ui' : 'observe-ui'}.log.txt',
      ).writeAsString(
        '${DateTime.now().toUtc().toIso8601String()} $event\n',
        mode: FileMode.append,
      );
    } catch (_) {
    } finally {
      logBusy = false;
    }
  }

  bool get connected => socket != null;
  List<Map<String, dynamic>> get students {
    final current = <String, Map<String, dynamic>>{};
    for (final r in (state['students'] as List? ?? [])) {
      current[r['id'] as String] = Map<String, dynamic>.from(r as Map);
    }
    final result = <Map<String, dynamic>>[
      for (final r in known.values)
        {
          ...r,
          'online': false,
          'monitoring': 'offline',
          ...?current.remove(r['id']),
        },
      ...current.values,
    ];
    for (final r in result) {
      if (!connected) {
        r['online'] = false;
      }
    }
    result.sort((a, b) {
      final status =
          (b['online'] == true ? 1 : 0) - (a['online'] == true ? 1 : 0);
      if (status != 0) {
        return status;
      }
      final name = '${a['name']}'.toLowerCase().compareTo(
        '${b['name']}'.toLowerCase(),
      );
      if (name != 0) {
        return name;
      }
      final ai = InternetAddress.tryParse('${a['ip']}')?.rawAddress;
      final bi = InternetAddress.tryParse('${b['ip']}')?.rawAddress;
      if (ai != null && bi != null && ai.length == bi.length) {
        for (var i = 0; i < ai.length; i++) {
          if (ai[i] != bi[i]) {
            return ai[i].compareTo(bi[i]);
          }
        }
      }
      return '${a['ip']}'.compareTo('${b['ip']}');
    });
    return result;
  }

  String? option(String key) {
    final i = args.indexOf(key);
    return i >= 0 && i + 1 < args.length ? args[i + 1] : null;
  }

  Future<void> loadKnown() async {
    if (known.isEmpty) {
      final cache = File('${dataDirectory!.path}/known-students.json');
      try {
        if (await cache.exists()) {
          final v =
              jsonDecode(await cache.readAsString()) as Map<String, dynamic>;
          roomKey = v['room'] as String;
          for (final row in (v['students'] as List).take(256)) {
            final r = Map<String, dynamic>.from(row as Map);
            known[r['id'] as String] = r;
          }
        }
      } catch (_) {
        await log('ignored invalid roster cache');
      }
    }
  }

  Future<void> start() async {
    if (disposed || shutdownReceived) return;
    try {
      var dir = await getApplicationSupportDirectory();
      if (Platform.isMacOS) {
        dir = await existingMacConsoleDirectory(
          dir,
          Platform.environment['HOME'],
        );
      }
      await dir.create(recursive: true);
      if (dataDirectory == null) {
        if (!dependent) {
          await File('${dir.path}/ui.log.txt').writeAsString('');
        }
      }
      dataDirectory = dir;
      await loadKnown();
      if (!disposed) notifyListeners();
      await log('console startup');
      final saved = File('${dir.path}/console-settings.json');
      if (await saved.exists()) {
        preferences =
            jsonDecode(await saved.readAsString()) as Map<String, dynamic>;
      }
      endpoint = File(
        option('--ipc-file') ?? '${dir.path}/teacher-endpoint.json',
      );
      final lock = await File(
        '${endpoint.path}.lock',
      ).open(mode: FileMode.append);
      await lock.lock();
      try {
        if (await endpoint.exists()) {
          try {
            final descriptor =
                jsonDecode(await endpoint.readAsString())
                    as Map<String, dynamic>;
            if (windowsProcessIsAlive(descriptor['pid'] as int?) == false) {
              // A dead owner can leave an IPC port that times out instead of
              // refusing the connection. Do not mistake a paused live helper
              // for a dead one and launch a second Teacher.
              await endpoint.delete();
            } else {
              await attach();
              return;
            }
          } on SocketException catch (e) {
            socket?.destroy();
            socket = null;
            if (![61, 111, 10061].contains(e.osError?.errorCode)) {
              rethrow;
            }
            await endpoint.delete();
          } on FormatException {
            // A descriptor left by an interrupted older build is not usable.
            // Startup is serialized by the endpoint lock; remove only this file.
            await endpoint.delete();
          }
        }
        // Observe is a dependent window. It must never resurrect a Teacher
        // after Exit or start a new room when its endpoint has disappeared.
        if (dependent) {
          await finishApplication();
          return;
        }
        final executable = File(Platform.resolvedExecutable);
        final macHelper = File(
          '${executable.parent.parent.path}/Helpers/Classroom Agent.app/Contents/MacOS/Classroom',
        );
        final helper =
            option('--agent-path') ??
            Platform.environment['CLASSROOM_AGENT_PATH'] ??
            (Platform.isMacOS
                ? await macHelper.exists()
                      ? macHelper.path
                      : '${executable.parent.parent.path}/Helpers/Classroom'
                : '${executable.parent.path}/ClassroomAgent.exe');
        if (!await File(helper).exists()) {
          throw StateError('Rust agent not found: $helper');
        }
        await Process.start(helper, [
          if (await File('${dir.path}/teacher-settings.toml').exists()) ...[
            '--config',
            '${dir.path}/teacher-settings.toml',
          ],
          '--role',
          'teacher',
          '--teacher-agent',
          '--ipc-file',
          endpoint.path,
          '--console-path',
          Platform.resolvedExecutable,
          '--log-file',
          option('--log-file') ?? '${dir.path}/teacher-log.txt',
          '--launch-profile',
          '${dir.path}/teacher-profile.toml',
          '--bind-ip',
          option('--bind-ip') ?? preferences['bind_ip'] as String? ?? 'auto',
          '--control-port',
          option('--control-port') ?? '${preferences['port'] ?? 47800}',
          '--media-port',
          '${int.parse(option('--control-port') ?? '${preferences['port'] ?? 47800}') + 10}',
          '--feedback-port',
          '${int.parse(option('--control-port') ?? '${preferences['port'] ?? 47800}') + 11}',
        ], mode: ProcessStartMode.detached);
        for (var i = 0; i < 100; i++) {
          if (await endpoint.exists()) {
            await attach();
            return;
          }
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        throw StateError(
          'Teacher agent did not start. See ${dir.path}/teacher-log.txt',
        );
      } finally {
        await lock.unlock();
        await lock.close();
      }
    } catch (e) {
      error = e.toString();
      await log('console startup failed: $e');
      notifyListeners();
    }
  }

  void send(Map<String, dynamic> value) {
    final bytes = utf8.encode(jsonEncode(value));
    final header = ByteData(4)..setUint32(0, bytes.length);
    socket!.add(header.buffer.asUint8List());
    socket!.add(bytes);
  }

  Future<void> attach() async {
    lastObserveFocus = 0;
    pendingObserveFocus = null;
    final descriptor =
        jsonDecode(await endpoint.readAsString()) as Map<String, dynamic>;
    final s = await Socket.connect(
      InternetAddress.loopbackIPv4,
      descriptor['port'] as int,
      timeout: const Duration(seconds: 2),
    );
    final previous = socket;
    socket = s;
    requestedThumbnailWidth = null;
    sentThumbnailWidth = null;
    previous?.destroy();
    s.setOption(SocketOption.tcpNoDelay, true);
    final ready = Completer<void>();
    var buffer = <int>[];
    s.listen(
      (chunk) {
        if (!identical(socket, s)) return;
        buffer.addAll(chunk);
        try {
          while (buffer.length >= 4) {
            final n = ByteData.sublistView(
              Uint8List.fromList(buffer.sublist(0, 4)),
            ).getUint32(0);
            if (n == 0 || n > 8 * 1024 * 1024) {
              throw const FormatException('Invalid IPC frame');
            }
            if (buffer.length < n + 4) {
              break;
            }
            final v =
                jsonDecode(utf8.decode(buffer.sublist(4, n + 4)))
                    as Map<String, dynamic>;
            buffer = buffer.sublist(n + 4);
            if (v['type'] == 'state') {
              ingestState(v);
              error = v['error'] as String?;
              if (!ready.isCompleted) {
                ready.complete();
              }
              final lifecycle = v['lifecycle'];
              if (lifecycle == 'exit' ||
                  (lifecycle == 'reconfigure' && dependent)) {
                unawaited(finishApplication());
              } else if (lifecycle == 'restart' || lifecycle == 'reconfigure') {
                // Main console reconnects to the replacement. Observe leases
                // belong to the old agent and their windows close with it.
                if (dependent) {
                  unawaited(finishApplication());
                }
              }
            } else {
              pending.remove(v['id'])?.complete(v);
            }
            if (!disposed) {
              notifyListeners();
            }
          }
        } catch (e) {
          error = e.toString();
          s.destroy();
        }
      },
      onDone: () {
        // An older IPC connection may finish after its replacement attached.
        // Its callback must not mark every live Student offline in the new UI.
        if (!identical(socket, s)) return;
        socket = null;
        if (!ready.isCompleted) {
          ready.completeError(StateError('Agent disconnected'));
        }
        if (dependent && !shutdownReceived) {
          unawaited(finishApplication());
        } else if (!disposed && !shutdownReceived) {
          notifyListeners();
          if (!exiting) {
            reconnect?.cancel();
            thumbnailSizeTimer?.cancel();
            reconnect = Timer(const Duration(seconds: 2), start);
          }
        }
      },
      onError: (Object e) {
        if (!identical(socket, s)) return;
        error = e.toString();
        if (!ready.isCompleted) {
          ready.completeError(e);
        }
      },
    );
    send({
      'type': 'hello',
      'protocol': 1,
      'token': descriptor['token'],
      'ui_role': !dependent ? 'console' : 'observe',
    });
    await ready.future.timeout(const Duration(seconds: 3));
    await log('IPC attached');
    notifyListeners();
  }

  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) => _command(action, selected, input, false);

  /// File staging must stop if the Hub rejects a submission or loses its socket.
  Future<void> commandChecked(
    String action,
    Iterable<String> selected,
    Map<String, dynamic> input,
  ) => _command(action, selected, input, true);

  Future<void> _command(
    String action,
    Iterable<String> selected,
    Map<String, dynamic>? input,
    bool throwOnError,
  ) async {
    if (socket == null) {
      if (throwOnError) throw StateError('Teacher agent is not connected.');
      return;
    }
    if (action == 'quit') {
      exiting = true;
    }
    commandBusy = true;
    notifyListeners();
    final elapsed = Stopwatch()..start();
    final id = ++serial;
    final c = Completer<Map<String, dynamic>>();
    pending[id] = c;
    try {
      send({
        'id': id,
        'action': action,
        'selected': selected.toList(),
        'input': ?input,
      });
      final result = await c.future.timeout(const Duration(seconds: 5));
      if (result['ok'] != true) {
        throw StateError('${result['error']}');
      }
    } catch (e) {
      error = e.toString();
      notifyListeners();
      if (throwOnError) rethrow;
    } finally {
      commandBusy = false;
      if (!disposed) notifyListeners();
      pending.remove(id);
      await log(
        'command action=$action round_trip_ms=${elapsed.elapsedMilliseconds}',
      );
    }
  }

  /// Like [command] but returns the whole acknowledgement (`ok`, `error`,
  /// `job_id`, `result`) and never throws on a refused command.
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    if (socket == null) {
      return {'ok': false, 'error': 'Teacher agent is not connected.'};
    }
    final id = ++serial;
    final c = Completer<Map<String, dynamic>>();
    pending[id] = c;
    try {
      send({
        'id': id,
        'action': action,
        'selected': selected.toList(),
        'input': ?input,
      });
      return await c.future.timeout(timeout);
    } catch (e) {
      return {'ok': false, 'error': '$e'};
    } finally {
      pending.remove(id);
    }
  }

  /// Human-readable failure of a [request] result.
  static String failure(Map<String, dynamic> result) {
    final detail = (result['result'] as Map?)?['error_detail'];
    return '${detail ?? result['error'] ?? 'Operation failed'}';
  }

  /// Opens (or focuses) the two-panel file window for one Student.
  Future<void> openFiles(Map<String, dynamic> row) async {
    if (row['online'] != true) {
      error = 'Student is offline';
      notifyListeners();
      return;
    }
    pendingObserveFocus = 'files:${row['id']}';
    final result = await request('files_window', selected: ['${row['id']}']);
    if (result['ok'] != true) {
      pendingObserveFocus = null;
      error = failure(result);
      notifyListeners();
    }
  }

  /// Opens (or focuses) the dedicated Activity window for one Student.
  Future<void> openActivity(Map<String, dynamic> row) async {
    if (row['online'] != true) {
      error = 'Student is offline';
      notifyListeners();
      return;
    }
    pendingObserveFocus = 'activity:${row['id']}';
    final result = await request('activity_window', selected: ['${row['id']}']);
    if (result['ok'] != true) {
      pendingObserveFocus = null;
      error = failure(result);
      notifyListeners();
    }
  }

  /// File jobs reported by the Rust hub (collect / fetch / push).
  List<Map<String, dynamic>> get fileJobs =>
      ((state['tools'] as Map? ?? const {})['fs_jobs'] as List? ?? const [])
          .map((e) => Map<String, dynamic>.from(e as Map))
          .toList();

  Future<void> openObserve(Map<String, dynamic> row) async {
    if (row['online'] != true) {
      error = 'Student is offline';
      notifyListeners();
      return;
    }
    final launch = ++observeLaunch;
    pendingObserveFocus = '${row['id']}';
    notice = 'Opening Observe · ${row['name']}';
    notifyListeners();
    String? previewPath;
    try {
      previewPath = await stageObservePreview(row);
    } catch (e) {
      await log('Observe preview staging failed: $e');
    }
    final bounds = await windowManager.getBounds();
    await command(
      'observe_window',
      ['${row['id']}'],
      {
        'center_x': bounds.center.dx,
        'center_y': bounds.center.dy,
        if (previewPath != null) 'preview_path': previewPath,
      },
    );
    // Give immediate feedback during native engine startup. Repeated clicks
    // are coalesced by the persistent Rust coordinator and focus its window.
    Timer(const Duration(seconds: 3), () {
      if (!disposed &&
          launch == observeLaunch &&
          notice?.startsWith('Opening Observe') == true) {
        notice = null;
        notifyListeners();
      }
    });
  }

  Future<String?> stageObservePreview(Map<String, dynamic> row) async {
    final id = '${row['id']}';
    final age = (row['thumbnail_age_ms'] as num?)?.toInt();
    final bytes = images[id];
    if (age == null || age > 10000 || bytes == null || bytes.isEmpty) {
      return null;
    }
    final directory = dataDirectory;
    if (directory == null) return null;
    final previews = Directory('${directory.path}/observe-previews');
    await previews.create(recursive: true);
    final safeId = id.replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '_');
    final file = File('${previews.path}/$safeId.jpg');
    await file.writeAsBytes(bytes, flush: true);
    return file.path;
  }

  // Small high-rate input bypasses command progress/ack waits. The socket, Rust
  // command queue and receiver pipe are bounded; failure ends the input lease.
  void observeInput(String id, Map<String, dynamic> input) {
    if (socket == null) return;
    if (input['action'] == 'move') {
      // Pointer motion is a state, not an event that must be replayed. Keep
      // only the newest position and flush it at a bounded high rate so TCP
      // cannot accumulate stale mouse coordinates.
      pendingObserveMoves[id] = input;
      observeMoveFlush ??= Timer.periodic(const Duration(milliseconds: 8), (_) {
        if (socket == null || pendingObserveMoves.isEmpty) {
          observeMoveFlush?.cancel();
          observeMoveFlush = null;
          return;
        }
        final pending = Map<String, Map<String, dynamic>>.from(
          pendingObserveMoves,
        );
        pendingObserveMoves.clear();
        for (final entry in pending.entries) {
          send({
            'id': 0,
            'action': 'observe_input',
            'selected': [entry.key],
            'input': entry.value,
          });
        }
      });
      return;
    }
    // A button/key barrier must never overtake the latest queued motion.
    final move = pendingObserveMoves.remove(id);
    if (move != null) {
      send({
        'id': 0,
        'action': 'observe_input',
        'selected': [id],
        'input': move,
      });
    }
    send({
      'id': 0,
      'action': 'observe_input',
      'selected': [id],
      'input': input,
    });
  }

  Future<void> savePreferences(Map<String, dynamic> changes) async {
    preferences.addAll(changes);
    if (dataDirectory == null) {
      return;
    }
    final f = File('${dataDirectory!.path}/console-settings.json');
    // The main console and every dependent window (Observe/Files/Activity/
    // Internet/VS Code) are separate processes sharing this one file with
    // no lock; each only loads it once, at its own startup. Serializing
    // this process's own in-memory `preferences` would silently erase
    // whatever any other window wrote afterward — confirmed: a long-running
    // console's stale copy was overwriting a dependent window's save within
    // seconds. Re-read the file fresh and merge `changes` onto *that*.
    Map<String, dynamic> onDisk = {};
    if (await f.exists()) {
      try {
        onDisk = jsonDecode(await f.readAsString()) as Map<String, dynamic>;
      } catch (_) {
        onDisk = {};
      }
    }
    onDisk.addAll(changes);
    preferences = onDisk;
    final temp = File('${f.path}.tmp');
    await temp.writeAsString(jsonEncode(preferences), flush: true);
    if (Platform.isWindows && await f.exists()) {
      await f.delete();
    }
    await temp.rename(f.path);
  }

  Future<void> configure(
    String ip,
    int port, [
    Map<String, dynamic>? settings,
  ]) async {
    if (state['phase'] != 'idle' && connected) {
      return;
    }
    await savePreferences({'bind_ip': ip, 'port': port});
    if (settings != null && dataDirectory != null) {
      final content = StringBuffer();
      for (final section in [
        'app',
        'network',
        'video',
        'monitoring',
        'diagnostics',
        'observe',
        'tools',
      ]) {
        final values = settings[section] as Map?;
        if (values == null) continue;
        content.writeln('[$section]');
        for (final entry in values.entries) {
          content.writeln('${entry.key} = ${jsonEncode(entry.value)}');
        }
      }
      final file = File('${dataDirectory!.path}/teacher-settings.toml');
      await file.writeAsString(content.toString(), flush: true);
    }
    reconfiguring = true;
    exiting = true;
    if (connected) {
      await command('reconfigure');
    }
    for (var i = 0; i < 100 && await endpoint.exists(); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
    if (await endpoint.exists()) {
      error = 'Agent still stopping; retry shortly.';
      notifyListeners();
      return;
    }
    exiting = false;
    reconfiguring = false;
    state = {};
    await start();
  }

  Future<void> finishApplication() async {
    if (shutdownReceived) return;
    shutdownReceived = true;
    exiting = true;
    reconnect?.cancel();
    thumbnailSizeTimer?.cancel();
    observeMoveFlush?.cancel();
    pendingObserveMoves.clear();
    await log('application Exit received; closing dependent window');
    await closeApplication?.call();
  }

  @override
  void dispose() {
    disposed = true;
    reconnect?.cancel();
    thumbnailSizeTimer?.cancel();
    observeMoveFlush?.cancel();
    observeMoveFlush = null;
    pendingObserveMoves.clear();
    socket?.destroy();
    super.dispose();
  }
}
