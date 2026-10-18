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
    'classroom.db',
    'teacher-profile.toml',
    'teacher-settings.toml',
    'console-settings.json',
  ]) {
    if (await File('${legacy.path}/$name').exists()) return legacy;
  }
  return suggested;
}

/// Delay before the next poll of a background job result. Starts short so a job
/// that finishes quickly is shown at once, then backs off to [ceiling] so a slow
/// Student never costs more requests than the old fixed interval did.
Duration resultPollDelay(int attempt, Duration ceiling) {
  final ms = 40 * (1 << attempt.clamp(0, 6));
  return Duration(
    milliseconds: ms < ceiling.inMilliseconds ? ms : ceiling.inMilliseconds,
  );
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

  /// The agent owns the single settings database; until the console has read
  /// it (right after attaching) it is not considered connected, so windows
  /// never read empty defaults.
  bool _storeLoaded = false;
  Map<String, dynamic> _unsavedPreferences = {};
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
      option('--hardware-control') != null ||
      option('--apps-ids') != null ||
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
    if (dirty) persistKnown();
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

  bool get connected => socket != null && _storeLoaded;
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

  // ------------------------------------------------------------ settings

  /// Every document of one namespace of the Teacher's settings database.
  Future<Map<String, dynamic>> storeList(String namespace) async {
    final response = await request('store_list', input: {'ns': namespace});
    if (response['ok'] != true) throw StateError(failure(response));
    return Map<String, dynamic>.from(
      ((response['result'] as Map?)?['documents'] as Map?) ?? const {},
    );
  }

  /// Writes documents (a `null` value removes the key). With [replace] the
  /// namespace becomes exactly [changes].
  Future<void> storeApply(
    String namespace,
    Map<String, dynamic> changes, {
    bool replace = false,
  }) async {
    final response = await request(
      'store_apply',
      input: {
        'ns': namespace,
        'changes': changes,
        'replace': ?(replace ? true : null),
      },
    );
    if (response['ok'] != true) throw StateError(failure(response));
  }

  void persistKnown() {
    if (!_storeLoaded) return;
    final changes = <String, dynamic>{
      '__room': roomKey,
      for (final entry in known.entries) entry.key: entry.value,
    };
    writes = writes
        .then((_) => storeApply('roster', changes, replace: true))
        .catchError((Object e) {
          log('roster save failed: $e');
        });
  }

  /// Reads preferences and the remembered roster once per attach.
  Future<void> loadStore() async {
    _storeLoaded = false;
    try {
      final prefs = await storeList('prefs');
      preferences = {...prefs, ..._unsavedPreferences}
        ..removeWhere((_, v) => v == null);
      final roster = await storeList('roster');
      final room = roster['__room'];
      if (room is String && (roomKey.isEmpty || room == roomKey)) {
        if (roomKey.isEmpty) roomKey = room;
        for (final entry in roster.entries.take(257)) {
          if (entry.key == '__room' || entry.value is! Map) continue;
          known.putIfAbsent(
            entry.key,
            () => Map<String, dynamic>.from(entry.value as Map),
          );
        }
      }
      if (_unsavedPreferences.isNotEmpty) {
        final pending = _unsavedPreferences;
        _unsavedPreferences = {};
        await storeApply('prefs', pending);
      }
    } catch (e) {
      await log('settings load failed: $e');
    }
    _storeLoaded = true;
    if (!dependent) await expireDailyRules();
    persistKnown();
    if (!disposed) notifyListeners();
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
      if (!disposed) notifyListeners();
      await log('console startup');
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
          // Adapter and ports come from the settings database the agent
          // opens itself; the options below are explicit overrides only.
          if (option('--bind-ip') != null) ...[
            '--bind-ip',
            option('--bind-ip')!,
          ],
          if (option('--control-port') != null) ...[
            '--control-port',
            option('--control-port')!,
            '--media-port',
            '${int.parse(option('--control-port')!) + 10}',
            '--feedback-port',
            '${int.parse(option('--control-port')!) + 11}',
          ],
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
    await loadStore();
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
    final text = '${detail ?? result['error'] ?? ''}';
    // Raw transport errors are meaningless to a Teacher; say what happened and
    // what to do. Specific Student-supplied details pass through unchanged.
    if (text.contains('TimeoutException')) {
      return 'Hết thời gian chờ phản hồi. Máy học viên có thể đang bận, mạng chậm hoặc đã mất kết nối.';
    }
    if (text == 'Student is offline') return 'Máy học viên đang offline.';
    if (text.contains('not connected')) {
      return 'Chưa kết nối với chương trình giảng viên. Hãy thử lại sau giây lát.';
    }
    return text.isEmpty ? 'Thao tác không thành công. Hãy thử lại.' : text;
  }

  /// Opens (or focuses) the two-panel file window for one Student.
  Future<void> openFiles(Map<String, dynamic> row) async {
    if (row['online'] != true) {
      error = 'Máy học viên đang offline.';
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
      error = 'Máy học viên đang offline.';
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
      error = 'Máy học viên đang offline.';
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

  /// Settings that describe today's classroom rules; the Students void their
  /// side at midnight, so the Teacher's remembered selections do too.
  static const _dailyRuleKeys = [
    'internet_last',
    'vscode_enabled',
    'vscode_allowed',
    'copilot_network_block',
    'apps_policy',
  ];

  Future<void> expireDailyRules() async {
    final now = DateTime.now();
    final today = now.year * 10000 + now.month * 100 + now.day;
    if (preferences['rules_day'] == today) return;
    final hadRules = _dailyRuleKeys.any(preferences.containsKey);
    // Dependent windows read the file at start; only the main console owns
    // the rewrite, so two processes never race on it.
    if (dependent) return;
    final changes = <String, dynamic>{'rules_day': today};
    for (final key in _dailyRuleKeys) {
      changes[key] = null;
    }
    if (hadRules || preferences['rules_day'] == null) {
      await savePreferences(changes);
    }
  }

  /// Remembers console preferences in the settings database. Changes made
  /// while the agent is unreachable are kept and written on the next attach.
  Future<void> savePreferences(Map<String, dynamic> changes) async {
    preferences = {...preferences, ...changes}
      ..removeWhere((_, v) => v == null);
    if (!_storeLoaded) {
      _unsavedPreferences = {..._unsavedPreferences, ...changes};
      return;
    }
    try {
      await storeApply('prefs', changes);
    } catch (e) {
      _unsavedPreferences = {..._unsavedPreferences, ...changes};
      await log('preference save failed: $e');
    }
  }

  /// Stores new settings and applies them to the running Teacher at once, with
  /// no restart. Only for settings the Teacher can take while running.
  Future<void> applySettings(Map<String, dynamic> overlay) async {
    await storeApply('config', {'overlay': overlay});
    await commandChecked('settings_apply', const [], const {});
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
    if (settings != null && connected) {
      final overlay = <String, dynamic>{
        for (final section in [
          'app',
          'network',
          'video',
          'monitoring',
          'diagnostics',
          'observe',
          'tools',
          'performance',
          'recording',
          'class',
          'activity',
          'transfer',
        ])
          if (settings[section] is Map) section: settings[section],
      };
      await storeApply('config', {'overlay': overlay});
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
