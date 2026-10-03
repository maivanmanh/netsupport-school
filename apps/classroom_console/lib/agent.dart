import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';
import 'package:window_manager/window_manager.dart';

class Agent extends ChangeNotifier {
  Agent(this.args);
  final List<String> args;
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

  void ingestState(Map<String, dynamic> value) {
    final key = '${value['teacher_ip']}:${value['control_port']}';
    if (roomKey != key) {
      known.clear();
      seenOnlineIds.clear();
      images.clear();
      roomKey = key;
    }
    state = value;
    var dirty = false;
    for (final row in (value['students'] as List? ?? [])) {
      final r = Map<String, dynamic>.from(row as Map);
      final id = r['id'] as String;
      if (r['online'] == true) {
        seenOnlineIds.add(id);
      }
      final old = known[id];
      if (old == null || old['name'] != r['name'] || old['ip'] != r['ip']) {
        dirty = true;
      }
      known[id] = {'id': id, 'name': r['name'], 'ip': r['ip']};
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
        '${dataDirectory!.path}/${option('--observe-id') == null ? 'ui' : 'observe-ui'}.log.txt',
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
    try {
      final dir = await getApplicationSupportDirectory();
      await dir.create(recursive: true);
      if (dataDirectory == null) {
        if (option('--observe-id') == null) {
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
            await attach();
            return;
          } on SocketException catch (e) {
            socket?.destroy();
            socket = null;
            if (![61, 111, 10061].contains(e.osError?.errorCode)) {
              rethrow;
            }
            await endpoint.delete();
          }
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
          '${dir.path}/teacher-log.txt',
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
    final descriptor =
        jsonDecode(await endpoint.readAsString()) as Map<String, dynamic>;
    final s = await Socket.connect(
      InternetAddress.loopbackIPv4,
      descriptor['port'] as int,
      timeout: const Duration(seconds: 2),
    );
    final previous = socket;
    socket = s;
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
        if (!disposed) {
          notifyListeners();
          if (!exiting) {
            reconnect?.cancel();
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
      'ui_role': option('--observe-id') == null ? 'console' : 'observe',
    });
    await ready.future.timeout(const Duration(seconds: 3));
    await log('IPC attached');
    notifyListeners();
  }

  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {
    if (socket == null) {
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
    } finally {
      commandBusy = false;
      if (!disposed) notifyListeners();
      pending.remove(id);
      await log(
        'command action=$action round_trip_ms=${elapsed.elapsedMilliseconds}',
      );
    }
  }

  Future<void> openObserve(Map<String, dynamic> row) async {
    if (row['online'] != true) {
      error = 'Student is offline';
      notifyListeners();
      return;
    }
    final bounds = await windowManager.getBounds();
    await Process.start(Platform.resolvedExecutable, [
      '--observe-id',
      '${row['id']}',
      '--observe-name',
      '${row['name']}',
      '--ipc-file',
      endpoint.path,
      '--teacher-center-x',
      '${bounds.center.dx}',
      '--teacher-center-y',
      '${bounds.center.dy}',
    ], mode: ProcessStartMode.detached);
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
    final temp = File('${f.path}.tmp');
    await temp.writeAsString(jsonEncode(preferences), flush: true);
    if (Platform.isWindows && await f.exists()) {
      await f.delete();
    }
    await temp.rename(f.path);
  }

  Future<void> configure(String ip, int port) async {
    if (state['phase'] != 'idle' && connected) {
      return;
    }
    await savePreferences({'bind_ip': ip, 'port': port});
    if (connected) {
      await command('quit');
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
    state = {};
    await start();
  }

  @override
  void dispose() {
    disposed = true;
    reconnect?.cancel();
    observeMoveFlush?.cancel();
    observeMoveFlush = null;
    pendingObserveMoves.clear();
    socket?.destroy();
    super.dispose();
  }
}
