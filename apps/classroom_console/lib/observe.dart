import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui' as ui;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:window_manager/window_manager.dart';
import 'agent.dart';
import 'observe_stats.dart';
import 'classroom_tools_ui.dart';
import 'file_transfer.dart';

import 'package:desktop_drop/desktop_drop.dart';

/// An independent native desktop window/process; never modal to the console.
class ObserveWindow extends StatefulWidget {
  const ObserveWindow({
    super.key,
    required this.agent,
    required this.studentId,
    required this.name,
  });
  final Agent agent;
  final String studentId, name;
  @override
  State<ObserveWindow> createState() => _ObserveWindowState();
}

class _ObserveWindowState extends State<ObserveWindow> with WindowListener {
  static const native = MethodChannel('classroom/observe');
  static const files = MethodChannel('classroom/files');
  Timer? tick, inputHeartbeat;
  late final ToolActivity toolActivity = ToolActivity(widget.agent);
  bool checkingFocus = false;
  bool started = false, closing = false, attaching = false;
  String? path, nativeError, requestedMode;
  String? modeError;
  Timer? reconnectTimer, modeRequestTimer;
  int reconnectAttempts = 0;
  bool reconnecting = false;
  String? lastStateSignature;
  int? texture;
  Map<String, dynamic> stats = {};
  final focus = FocusNode();
  final screenshotBoundary = GlobalKey();
  final viewportScroll = ScrollController();
  final horizontalScroll = ScrollController();
  Timer? chromeTimer;
  String displayMode = 'fill';
  bool autoHide = true, chromeVisible = true, fullScreen = false;
  bool overChrome = false;
  Offset? pointerPosition;
  final localCursor = ValueNotifier<Offset?>(null);
  ui.Image? remoteCursor;
  String? cursorKey;
  int cursorGeneration = 0;
  double cursorFraction = .015, cursorHotX = 0, cursorHotY = 0;

  void syncCursor() {
    final shape = view['cursor'];
    if (shape is! Map || !controls) return;
    final key = '${view['surface']}:${shape['serial']}';
    if (key == cursorKey) return;
    cursorKey = key;
    final generation = ++cursorGeneration;
    try {
      final width = (shape['width'] as num).toInt();
      final height = (shape['height'] as num).toInt();
      final pixels = base64Decode(shape['bgra'] as String);
      if (width < 1 ||
          width > 96 ||
          height < 1 ||
          height > 96 ||
          pixels.length != width * height * 4) {
        return;
      }
      ui.decodeImageFromPixels(pixels, width, height, ui.PixelFormat.bgra8888, (
        image,
      ) {
        if (!mounted || generation != cursorGeneration) {
          image.dispose();
          return;
        }
        final old = remoteCursor;
        setState(() {
          remoteCursor = image;
          cursorFraction =
              ((shape['width_fraction'] as num?)?.toDouble() ?? .015).clamp(
                .001,
                .25,
              );
          cursorHotX = ((shape['hotspot_x'] as num?)?.toDouble() ?? 0) / width;
          cursorHotY = ((shape['hotspot_y'] as num?)?.toDouble() ?? 0) / height;
        });
        old?.dispose();
      });
    } catch (_) {
      /* Keep the local arrow if remote shape is unavailable. */
    }
  }

  int pressedButtons = 0;
  final pressedKeys = <String>{};
  bool windowFocused = true;
  bool? capturingSystemKeys;
  int lastFocusRequest = 0;

  void syncSystemKeys() {
    if (!Platform.isWindows) return;
    final enabled = !closing && controls;
    if (capturingSystemKeys == enabled) return;
    capturingSystemKeys = enabled;
    unawaited(
      native.invokeMethod<void>('captureSystemKeys', enabled).catchError((
        Object error,
      ) {
        capturingSystemKeys = null;
        unawaited(widget.agent.log('Windows shortcut capture failed: $error'));
      }),
    );
  }

  Future<void> nativeKey(MethodCall call) async {
    if (call.method != 'systemKey' || !controls) return;
    final value = Map<String, dynamic>.from(call.arguments as Map);
    final label = value['key'] as String;
    final down = value['down'] == true;
    if (down) {
      pressedKeys.add(label);
    } else {
      pressedKeys.remove(label);
    }
    input({'action': 'key', 'key': label, 'down': down});
  }

  double scrollRemainder = 0;
  String? lastWindowTitle;
  bool showStats = false;
  bool preparingFile = false;
  bool draggingFiles = false;
  Future<void> dropFiles(List<DropItem> items) async {
    if (preparingFile || closing || items.isEmpty) return;
    releaseHeldInput();
    setState(() {
      preparingFile = true;
      draggingFiles = false;
    });
    final limit =
        ((widget.agent.state['settings'] as Map? ?? {})['tools'] as Map? ??
                {})['max_file_mb']
            as num? ??
        300;
    try {
      await sendDroppedFiles(
        items,
        outbox: '${widget.agent.endpoint.parent.path}/outbox',
        limit: limit.toInt() * 1024 * 1024,
        active: () => !closing,
        submit: (staged) => widget.agent.commandChecked('tool_file', [
          widget.studentId,
        ], staged),
      );
    } catch (e) {
      if (mounted && !closing) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('Could not send: $e')));
      }
    } finally {
      if (mounted) setState(() => preparingFile = false);
    }
  }

  double? previewFps;
  int? previousPreviewFrames;
  final previewClock = Stopwatch()..start();
  Duration? previousPreviewTime;
  bool polling = false;
  bool fastPolling = true, firstFrameLogged = false;
  DateTime? fastPollUntil;

  void startPolling({required bool fast}) {
    tick?.cancel();
    fastPolling = fast;
    fastPollUntil = fast
        ? DateTime.now().add(const Duration(seconds: 10))
        : null;
    tick = Timer.periodic(
      Duration(milliseconds: fast ? 100 : 1000),
      (_) => poll(),
    );
  }

  void syncWindowTitle() {
    final controlling = mode != 'observe' && view['phase'] == 'live';
    final title = '${controlling ? 'Control' : 'Observe'} · ${widget.name}';
    if (title == lastWindowTitle) return;
    lastWindowTitle = title;
    unawaited(windowManager.setTitle(title));
  }

  void scrollRemote(double pixels) {
    if (!controls || !pixels.isFinite) return;
    // Trackpads produce small deltas. Preserve the fraction instead of letting
    // the Student round each small event to zero lines.
    final lines = scrollRemainder + pixels / 24;
    final whole = lines.truncate();
    scrollRemainder = lines - whole;
    final delta = whole.clamp(-20, 20);
    if (delta != 0) input({'action': 'scroll', 'y': delta});
  }

  void revealChrome() {
    chromeTimer?.cancel();
    if (!chromeVisible && mounted) setState(() => chromeVisible = true);
    if (autoHide && !overChrome) {
      chromeTimer = Timer(const Duration(seconds: 2), () {
        if (mounted && autoHide && !overChrome) {
          setState(() => chromeVisible = false);
        }
      });
    }
  }

  void chromeHover(bool entered) {
    overChrome = entered;
    revealChrome();
  }

  void move(PointerEvent e, Size image, {Size? source, BoxFit? fit}) {
    if (!controls) return;
    final point = observePosition(
      e.localPosition,
      image,
      source: source,
      fit: fit,
    );
    pointerPosition = point;
    localCursor.value = e.localPosition;
    input({'action': 'move', 'x': point.dx, 'y': point.dy});
  }

  void updateButtons(int buttons) {
    if (!controls) return;
    const bindings = [
      (kPrimaryMouseButton, 'left'),
      (kSecondaryMouseButton, 'right'),
      (kMiddleMouseButton, 'middle'),
    ];
    final next =
        buttons &
        (kPrimaryMouseButton | kSecondaryMouseButton | kMiddleMouseButton);
    for (final (bit, button) in bindings) {
      final down = next & bit != 0;
      if (down == (pressedButtons & bit != 0)) continue;
      input({
        'action': pointerPosition == null ? 'button' : 'button_at',
        'key': button,
        'down': down,
        if (pointerPosition != null) 'x': pointerPosition!.dx,
        if (pointerPosition != null) 'y': pointerPosition!.dy,
      });
    }
    pressedButtons = next;
  }

  void releaseButtons() {
    updateButtons(0);
    pressedButtons = 0;
  }

  void releaseHeldInput() {
    releaseButtons();
    for (final key in pressedKeys) {
      input({'action': 'key', 'key': key, 'down': false});
    }
    pressedKeys.clear();
    scrollRemainder = 0;
  }

  int frames = 0;
  DateTime lastStats = DateTime.now();
  String get mode => widget.agent.connected
      ? (view['mode'] as String? ?? 'observe')
      : 'observe';
  Map<String, dynamic> get view {
    final value = widget.agent.state['observe'];
    return value is Map && value['id'] == widget.studentId
        ? Map<String, dynamic>.from(value)
        : {};
  }

  bool get live =>
      widget.agent.connected &&
      view['phase'] == 'live' &&
      (stats['sequence'] as num? ?? 0) > 0;
  bool get canSwitchMode => live;

  /// Negotiated codec for the footer (also shown in Session stats).
  String? get sessionCodec => ObserveStatsData.codecLabel(
    Map<String, dynamic>.from(view['metrics'] as Map? ?? const {}),
  );
  bool get controls =>
      windowFocused && live && mode != 'observe' && requestedMode == null;

  Future<void> saveStudentScreenshot() async {
    try {
      String? path;
      if (Platform.isWindows) {
        final renderObject = screenshotBoundary.currentContext
            ?.findRenderObject();
        if (renderObject is! RenderRepaintBoundary) {
          throw PlatformException(
            code: 'screenshot_unavailable',
            message: 'The student screen is not ready yet.',
          );
        }
        final image = await renderObject.toImage(pixelRatio: 1);
        try {
          final png = await image.toByteData(format: ui.ImageByteFormat.png);
          if (png == null) {
            throw PlatformException(
              code: 'screenshot_encode',
              message: 'Could not encode the screenshot.',
            );
          }
          path = await files.invokeMethod<String>('saveScreenshot', {
            'name': widget.name,
            'png': png.buffer.asUint8List(png.offsetInBytes, png.lengthInBytes),
          });
        } finally {
          image.dispose();
        }
      } else {
        path = await files.invokeMethod<String>('screenshot', {
          'name': widget.name,
        });
      }
      if (!mounted || path == null) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            'Screenshot saved to Desktop · ${path.split(RegExp(r'[/\\]')).last}',
          ),
        ),
      );
    } on PlatformException catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(e.message ?? 'Could not save screenshot')),
        );
      }
    }
  }

  Future<void> messageStudent() async {
    final controller = TextEditingController();
    final text = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('Message ${widget.name}'),
        content: SizedBox(
          width: 400,
          child: TextField(
            controller: controller,
            autofocus: true,
            maxLines: 5,
            maxLength: 2000,
            decoration: const InputDecoration(
              hintText: 'Write a message',
              border: OutlineInputBorder(),
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, controller.text),
            child: const Text('Send'),
          ),
        ],
      ),
    );
    if (text != null && text.trim().isNotEmpty) {
      await widget.agent.command(
        'tool_message',
        [widget.studentId],
        {'text': text.trim(), 'allow_reply': true},
      );
    }
    controller.dispose();
  }

  Future<void> sendFileToStudent() async {
    final directory = widget.agent.dataDirectory;
    if (directory == null) return;
    setState(() => preparingFile = true);
    try {
      final picked = await files.invokeMapMethod<String, dynamic>(
        'chooseFile',
        {'directory': widget.agent.endpoint.parent.path},
      );
      if (picked != null) {
        await widget.agent.command('tool_file', [widget.studentId], picked);
      }
    } on PlatformException catch (e) {
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(content: Text(e.message ?? 'Could not stage the file')),
        );
      }
    } finally {
      if (mounted) setState(() => preparingFile = false);
    }
  }

  @override
  void initState() {
    super.initState();
    toolActivity;
    if (Platform.isWindows) native.setMethodCallHandler(nativeKey);
    windowManager.addListener(this);
    windowManager.setPreventClose(true);
    widget.agent.addListener(changed);
    startPolling(fast: true);
    // Heartbeat must not depend on texture stats, attachment or video progress.
    inputHeartbeat = Timer.periodic(const Duration(milliseconds: 500), (_) {
      if (!closing && widget.agent.connected) {
        widget.agent.observeInput(widget.studentId, {'action': 'keepalive'});
        unawaited(reconcileFocus());
      }
    });
    revealChrome();
  }

  void changed() {
    syncCursor();
    if (closing) return;
    final request = widget.agent.state['observe_focus'] as List?;
    if (!closing &&
        request != null &&
        request.length >= 2 &&
        request[0] == widget.studentId &&
        request[1] != lastFocusRequest) {
      lastFocusRequest = request[1] as int;
      unawaited(() async {
        await windowManager.show();
        await windowManager.restore();
        await windowManager.focus();
      }());
    }
    if (!widget.agent.connected || mode == 'observe') {
      pressedButtons = 0;
      pressedKeys.clear();
      scrollRemainder = 0;
    }
    syncWindowTitle();
    syncSystemKeys();
    if (!started && widget.agent.connected) {
      started = true;
      unawaited(openObserve());
    }
    recordState();
    final source = view['surface'] as String?;
    if (source != null && source != path && !attaching) {
      modeError = null; // Errors belong to the old surface/session.
      cursorKey = null;
      ++cursorGeneration;
      remoteCursor?.dispose();
      remoteCursor = null;
      attach(source);
    }
    final viewError = view['error'] as String? ?? '';
    final modeAcknowledged = requestedMode == mode;
    if (modeAcknowledged || viewError.isNotEmpty) {
      modeRequestTimer?.cancel();
      requestedMode = null;
      // Session errors are rendered directly from view, never latched as a mode error.
      if (modeAcknowledged) modeError = null;
    }
    final phase = view['phase'];
    if ((phase == 'ended' || phase == 'error') &&
        widget.agent.connected &&
        reconnectAttempts < 2 &&
        reconnectTimer == null &&
        !closing) {
      reconnecting = true;
      reconnectTimer = Timer(const Duration(milliseconds: 800), () {
        reconnectTimer = null;
        if (!mounted || closing || !widget.agent.connected) return;
        if (view['phase'] != 'ended' && view['phase'] != 'error') return;
        reconnectAttempts++;
        unawaited(widget.agent.command('observe_open', [widget.studentId]));
      });
    } else if (phase == 'live') {
      reconnecting = false;
    }
    if (mounted) setState(() {});
  }

  Future<void> openObserve() async {
    await widget.agent.command('observe_open', [widget.studentId]);
    // The command acknowledgement is sent just before the next state
    // publication. Give that publication a chance, then explicitly retry the
    // surface lookup so a listener scheduling race cannot leave this window in
    // Connecting forever.
    await Future<void>.delayed(const Duration(milliseconds: 100));
    if (!closing && mounted) {
      recordState();
      final source = view['surface'] as String?;
      if (source != null && !attaching && (texture == null || source != path)) {
        await attach(source);
      }
      if (mounted) setState(() {});
    }
  }

  void recordState() {
    final current = view;
    final signature =
        '${current['id']}|${current['phase']}|${current['surface'] != null}|${current['error'] ?? ''}';
    if (signature == lastStateSignature) return;
    lastStateSignature = signature;
    unawaited(
      widget.agent.log(
        'Observe state phase=${current['phase'] ?? 'none'} surface=${current['surface'] != null} error=${current['error'] ?? ''}',
      ),
    );
  }

  Future<void> attach(String source) async {
    attaching = true;
    try {
      final id = await native.invokeMethod<int>('attach', {'path': source});
      if (id == null) {
        throw StateError(
          'Native observe texture attach returned no texture id',
        );
      }
      if (mounted) {
        setState(() {
          path = source;
          texture = id;
          nativeError = null;
          stats = {};
          previewFps = null;
          previousPreviewFrames = null;
          previousPreviewTime = null;
          firstFrameLogged = false;
        });
        startPolling(fast: true);
      }
      await widget.agent.log('Observe native texture attached');
    } catch (e) {
      if (mounted) setState(() => nativeError = '$e');
      await widget.agent.log('Observe native texture attach failed error=$e');
    } finally {
      attaching = false;
    }
  }

  Future<void> poll() async {
    if (closing || polling) return;
    polling = true;
    try {
      if (requestedMode != null && modeRequestTimer == null && mounted) {
        // Keep a fallback in case the mode acknowledgement is lost. Without
        // this, both mode controls stay disabled for the rest of the window.
        modeRequestTimer = Timer(const Duration(seconds: 4), () {
          modeRequestTimer = null;
          if (!mounted || requestedMode == null) return;
          setState(() {
            requestedMode = null;
            modeError = 'No response from student · Try the mode change again';
          });
        });
      }
      // State updates and the observe_open response can arrive in either order.
      // Polling here makes surface attachment resilient to that startup race and
      // retries a failed native attach without requiring the user to press Fit.
      final source = view['surface'] as String?;
      if (source != null && (texture == null || source != path) && !attaching) {
        await attach(source);
      }
      if (texture != null) {
        try {
          final value = await native
              .invokeMapMethod<String, dynamic>('stats')
              .timeout(const Duration(seconds: 1));
          if (mounted && value != null) {
            final count = (value['native_frames'] as num? ?? 0).toInt();
            final now = previewClock.elapsed;
            final previous = previousPreviewFrames;
            final before = previousPreviewTime;
            final seconds = before == null
                ? 0.0
                : (now - before).inMicroseconds / 1000000;
            setState(() {
              previewFps = previous != null && count >= previous && seconds > 0
                  ? (count - previous) / seconds
                  : null;
              previousPreviewFrames = count;
              previousPreviewTime = now;
              stats = value;
              nativeError = null;
            });
            if ((value['sequence'] as num? ?? 0) > 0 && !firstFrameLogged) {
              firstFrameLogged = true;
              await widget.agent.log(
                'Observe first native frame detected startup_ms=${previewClock.elapsedMilliseconds}',
              );
            }
          }
        } catch (e) {
          nativeError = '$e';
        }
        if (DateTime.now().difference(lastStats).inSeconds >= 5) {
          final count = (stats['native_frames'] as num? ?? 0).toInt();
          final elapsed =
              DateTime.now().difference(lastStats).inMilliseconds / 1000;
          await widget.agent.log(
            'Observe native_texture_fps=${((count - frames) / elapsed).toStringAsFixed(2)} sequence=${stats['sequence']} source_sequence=${stats['source_sequence']} connected=${stats['connected']} notifications=${stats['frame_notifications']} copy_attempts=${stats['copy_attempts']} copy_failures=${stats['copy_failures']} lock_misses=${stats['lock_misses']} cached_frame_returns=${stats['cached_frame_returns']} connect_error=${stats['connect_error']} width=${stats['width']} height=${stats['height']} mode=$mode layout=$displayMode fullscreen=$fullScreen focused=$windowFocused controls=$controls requested_mode=$requestedMode pool_misses=${stats['pool_misses']} chrome_auto_hide=$autoHide',
          );
          frames = count;
          lastStats = DateTime.now();
        }
      }
    } finally {
      if (fastPolling &&
          !closing &&
          (firstFrameLogged ||
              view['phase'] == 'error' ||
              view['phase'] == 'ended' ||
              (fastPollUntil != null &&
                  DateTime.now().isAfter(fastPollUntil!)))) {
        startPolling(fast: false);
      }
      polling = false;
      syncSystemKeys();
    }
  }

  void input(Map<String, dynamic> value) {
    if (controls) widget.agent.observeInput(widget.studentId, value);
  }

  void switchMode(String value) {
    if (!canSwitchMode || requestedMode != null) return;
    modeRequestTimer?.cancel();
    modeError = null;
    releaseHeldInput();
    pressedButtons = 0;
    scrollRemainder = 0;
    setState(() => requestedMode = value);
    syncSystemKeys();
    modeRequestTimer = Timer(const Duration(seconds: 4), () {
      modeRequestTimer = null;
      if (!mounted || requestedMode == null) return;
      setState(() {
        requestedMode = null;
        modeError = 'No response from student · Try the mode change again';
      });
    });
    widget.agent.observeInput(widget.studentId, {'action': value});
    focus.requestFocus();
  }

  @override
  Future<void> onWindowClose() async {
    if (closing) return;
    final surface = view['surface'];
    releaseHeldInput();
    inputHeartbeat?.cancel();
    tick?.cancel();
    setState(() {
      closing = true;
      texture = null;
    });
    syncSystemKeys();
    // Hide before awaiting IPC: a slow/offline Student must never leave a
    // visible empty window behind after the owner presses Close.
    await windowManager.hide();
    // Remove the Texture layer before pausing native notifications/teardown.
    try {
      await WidgetsBinding.instance.endOfFrame.timeout(
        const Duration(milliseconds: 250),
      );
      await native
          .invokeMethod<void>('detach')
          .timeout(const Duration(seconds: 1));
    } catch (e) {
      unawaited(widget.agent.log('Observe close detach error=$e'));
    } finally {
      // The reliable control socket already owns delivery; do not wait for
      // the remote acknowledgement before destroying this native window.
      unawaited(
        widget.agent.command(
          'observe_close',
          [widget.studentId],
          {'surface': surface},
        ),
      );
      try {
        await widget.agent.socket?.flush().timeout(
          const Duration(milliseconds: 100),
        );
      } catch (_) {
        /* The coordinator also expires disconnected input leases. */
      }
      if (widget.agent.closeApplication != null) {
        // This is a separate UI process. Use the same bounded native close and
        // explicit process exit as the application's lifecycle handler; posting
        // WM_QUIT alone can leave Dart/plugin threads and the registry alive.
        await widget.agent.finishApplication();
      } else {
        await windowManager.destroy();
      }
    }
  }

  Future<void> toggleFullScreen() async {
    final next = !fullScreen;
    if (next && displayMode != 'fill' && mounted) {
      setState(() => displayMode = 'fill');
    }
    await windowManager.setFullScreen(next);
    if (mounted) setState(() => fullScreen = next);
    revealChrome();
  }

  Future<void> reconcileFocus() async {
    if (checkingFocus || closing || !mounted) return;
    checkingFocus = true;
    try {
      final actual = await windowManager.isFocused().timeout(
        const Duration(seconds: 1),
      );
      if (!closing && mounted && actual != windowFocused) {
        if (actual) {
          onWindowFocus();
        } else {
          onWindowBlur();
        }
        unawaited(
          widget.agent.log(
            'Observe focus reconciled focused=$actual mode=$mode',
          ),
        );
      }
    } catch (_) {
      // A temporarily unavailable native query must not change session mode.
    } finally {
      checkingFocus = false;
    }
  }

  @override
  void onWindowBlur() {
    // Losing focus is not a mode change. Keep the session/lease alive while
    // releasing held input, so copying to another app cannot leave keys stuck.
    releaseHeldInput();
    windowFocused = false;
    focus.unfocus();
    syncSystemKeys();
    if (mounted) setState(() {});
  }

  @override
  void onWindowFocus() {
    windowFocused = true;
    syncSystemKeys();
    if (mode != 'observe') focus.requestFocus();
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    if (Platform.isWindows) {
      unawaited(native.invokeMethod<void>('captureSystemKeys', false));
      native.setMethodCallHandler(null);
    }
    toolActivity.dispose();
    tick?.cancel();
    inputHeartbeat?.cancel();
    chromeTimer?.cancel();
    reconnectTimer?.cancel();
    modeRequestTimer?.cancel();
    viewportScroll.dispose();
    horizontalScroll.dispose();
    widget.agent.removeListener(changed);
    windowManager.removeListener(this);
    focus.dispose();
    ++cursorGeneration;
    remoteCursor?.dispose();
    localCursor.dispose();
    super.dispose();
  }

  KeyEventResult key(FocusNode _, KeyEvent event) {
    if (!controls) return KeyEventResult.ignored;
    // Native hook owns Windows keys; ignore Flutter synthetic modifier repairs.
    if (Platform.isWindows &&
        (event.logicalKey == LogicalKeyboardKey.metaLeft ||
            event.logicalKey == LogicalKeyboardKey.metaRight)) {
      return KeyEventResult.handled;
    }
    final named = <LogicalKeyboardKey, String>{
      LogicalKeyboardKey.enter: 'Enter',
      LogicalKeyboardKey.backspace: 'Backspace',
      LogicalKeyboardKey.tab: 'Tab',
      LogicalKeyboardKey.escape: 'Escape',
      LogicalKeyboardKey.shiftLeft: 'Shift',
      LogicalKeyboardKey.shiftRight: 'Shift',
      LogicalKeyboardKey.controlLeft: 'Control',
      LogicalKeyboardKey.controlRight: 'Control',
      LogicalKeyboardKey.altLeft: 'Alt',
      LogicalKeyboardKey.altRight: 'Alt',
      LogicalKeyboardKey.metaLeft: 'Meta',
      LogicalKeyboardKey.metaRight: 'Meta',
      LogicalKeyboardKey.arrowLeft: 'ArrowLeft',
      LogicalKeyboardKey.arrowRight: 'ArrowRight',
      LogicalKeyboardKey.arrowUp: 'ArrowUp',
      LogicalKeyboardKey.arrowDown: 'ArrowDown',
      LogicalKeyboardKey.delete: 'Delete',
      LogicalKeyboardKey.home: 'Home',
      LogicalKeyboardKey.end: 'End',
      LogicalKeyboardKey.pageUp: 'PageUp',
      LogicalKeyboardKey.pageDown: 'PageDown',
    };
    final label =
        named[event.logicalKey] ?? event.logicalKey.keyLabel.toLowerCase();
    if (label.runes.length == 1 || named.containsKey(event.logicalKey)) {
      final down = event is! KeyUpEvent;
      if (down) {
        pressedKeys.add(label);
      } else {
        pressedKeys.remove(label);
      }
      input({'action': 'key', 'key': label, 'down': down});
    }
    return KeyEventResult.handled;
  }

  Widget chrome({required Alignment alignment, required Widget child}) {
    return Align(
      alignment: alignment,
      child: IgnorePointer(
        // Opacity alone does not remove a MouseRegion from hit testing. The
        // entire hidden toolbar must be absent from the pointer path.
        ignoring: !chromeVisible,
        child: MouseRegion(
          onEnter: (_) => chromeHover(true),
          onExit: (_) => chromeHover(false),
          child: AnimatedOpacity(
            opacity: chromeVisible ? 1 : 0,
            duration: const Duration(milliseconds: 180),
            child: Material(
              color: const Color(0xeff3f5f9),
              elevation: 2,
              child: child,
            ),
          ),
        ),
      ),
    );
  }

  Widget video(Size size, {Size? source, BoxFit? fit}) {
    Widget textureView() {
      if (fit == BoxFit.cover &&
          source != null &&
          source.width > 0 &&
          source.height > 0) {
        final scale = math.max(
          size.width / source.width,
          size.height / source.height,
        );
        final drawn = Size(source.width * scale, source.height * scale);
        final crop = Offset(
          (drawn.width - size.width) / 2,
          (drawn.height - size.height) / 2,
        );
        return ClipRect(
          child: SizedBox.expand(
            child: Stack(
              clipBehavior: Clip.hardEdge,
              children: [
                Positioned(
                  left: -crop.dx,
                  top: -crop.dy,
                  width: drawn.width,
                  height: drawn.height,
                  child: SizedBox(
                    width: drawn.width,
                    height: drawn.height,
                    child: Texture(
                      textureId: texture!,
                      filterQuality: FilterQuality.medium,
                    ),
                  ),
                ),
              ],
            ),
          ),
        );
      }
      return SizedBox(
        width: size.width,
        height: size.height,
        child: Texture(
          textureId: texture!,
          filterQuality: FilterQuality.medium,
        ),
      );
    }

    return RepaintBoundary(
      key: screenshotBoundary,
      child: SizedBox(
        key: const ValueKey('observe-image'),
        width: size.width,
        height: size.height,
        child: MouseRegion(
          // The OS cursor responds immediately, independently of video latency.
          // Click coordinates use the same transform as this local pointer.
          cursor: controls && remoteCursor != null
              ? SystemMouseCursors.none
              : SystemMouseCursors.basic,
          onExit: (_) => localCursor.value = null,
          child: Focus(
            focusNode: focus,
            onKeyEvent: key,
            child: Listener(
              behavior: HitTestBehavior.opaque,
              onPointerHover: (e) => move(e, size, source: source, fit: fit),
              onPointerMove: (e) {
                move(e, size, source: source, fit: fit);
                // Flutter reports additional button presses/releases during a
                // chord as move events, not a second down/up pointer lifecycle.
                updateButtons(e.buttons);
              },
              onPointerDown: (e) {
                if (!controls) return;
                focus.requestFocus();
                move(e, size, source: source, fit: fit);
                updateButtons(e.buttons);
              },
              onPointerUp: (e) {
                move(e, size, source: source, fit: fit);
                updateButtons(e.buttons);
              },
              onPointerCancel: (_) => releaseButtons(),
              onPointerPanZoomStart: (e) {
                if (!controls) return;
                scrollRemainder = 0;
                focus.requestFocus();
                move(e, size, source: source, fit: fit);
              },
              onPointerPanZoomUpdate: (e) {
                // Flutter desktop trackpads use pan/zoom events, not wheel
                // signals. Pan describes content movement, inverse to wheel.
                scrollRemote(-e.localPanDelta.dy);
              },
              onPointerSignal: (e) {
                if (controls && e is PointerScrollEvent) {
                  // Resolve here so the wheel cannot also move the local viewport.
                  GestureBinding.instance.pointerSignalResolver.register(e, (
                    _,
                  ) {
                    move(e, size, source: source, fit: fit);
                    scrollRemote(e.scrollDelta.dy);
                  });
                }
              },
              child: Stack(
                fit: StackFit.expand,
                children: [
                  if (texture != null) textureView(),
                  if (controls && remoteCursor != null)
                    Positioned.fill(
                      child: IgnorePointer(
                        child: RepaintBoundary(
                          child: ValueListenableBuilder<Offset?>(
                            valueListenable: localCursor,
                            builder: (context, position, _) {
                              if (position == null) {
                                return const SizedBox.shrink();
                              }
                              final width = (cursorFraction * size.width).clamp(
                                12.0,
                                96.0,
                              );
                              final height =
                                  width *
                                  remoteCursor!.height /
                                  remoteCursor!.width;
                              return Stack(
                                children: [
                                  Positioned(
                                    left: position.dx - cursorHotX * width,
                                    top: position.dy - cursorHotY * height,
                                    width: width,
                                    height: height,
                                    child: RawImage(
                                      image: remoteCursor,
                                      filterQuality: FilterQuality.medium,
                                    ),
                                  ),
                                ],
                              );
                            },
                          ),
                        ),
                      ),
                    ),
                  if (!live)
                    Center(
                      child: Text(
                        view['phase'] == 'error' || view['phase'] == 'ended'
                            ? 'Observe ended'
                            : 'Connecting to student…',
                        style: const TextStyle(color: Colors.white70),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final error =
        modeError ??
        nativeError ??
        view['error'] as String? ??
        widget.agent.error;
    return Scaffold(
      body: DropTarget(
        enable: !closing && !preparingFile,
        onDragEntered: (_) {
          releaseHeldInput();
          setState(() => draggingFiles = true);
        },
        onDragExited: (_) => setState(() => draggingFiles = false),
        onDragDone: (detail) => unawaited(dropFiles(detail.files)),
        child: Stack(
          children: [
            Positioned.fill(
              child: ColoredBox(
                color: const Color(0xff111b24),
                child: LayoutBuilder(
                  builder: (context, constraints) {
                    final source = Size(
                      (stats['width'] as num? ?? 16).toDouble(),
                      (stats['height'] as num? ?? 9).toDouble(),
                    );
                    final viewport = Size(
                      constraints.maxWidth,
                      constraints.maxHeight,
                    );
                    final fill = displayMode == 'fill';
                    final cover = displayMode == 'stretch';
                    final size = observeImageSize(
                      viewport,
                      source,
                      stretch: fill,
                    );
                    if (fill) {
                      return video(size, source: source, fit: BoxFit.fill);
                    }
                    if (cover) {
                      final safeSource = source.width > 0 && source.height > 0
                          ? source
                          : const Size(16, 9);
                      final scale = math.max(
                        viewport.width / safeSource.width,
                        viewport.height / safeSource.height,
                      );
                      final drawn = Size(
                        safeSource.width * scale,
                        safeSource.height * scale,
                      );
                      // Keep the whole image in the scrollable content. Pointer
                      // coordinates are relative to that image, never the crop.
                      // Reserve local scrollbar gutters outside the video:
                      // dragging a thumb must never send clicks to the Student.
                      const gutter = 12.0;
                      return Scrollbar(
                        controller: viewportScroll,
                        thumbVisibility: true,
                        notificationPredicate: (n) =>
                            n.metrics.axis == Axis.vertical,
                        child: Scrollbar(
                          controller: horizontalScroll,
                          thumbVisibility: true,
                          notificationPredicate: (n) =>
                              n.metrics.axis == Axis.horizontal,
                          child: Padding(
                            padding: const EdgeInsets.only(
                              right: gutter,
                              bottom: gutter,
                            ),
                            child: SingleChildScrollView(
                              controller: horizontalScroll,
                              scrollDirection: Axis.horizontal,
                              physics: controls
                                  ? const NeverScrollableScrollPhysics()
                                  : null,
                              child: SizedBox(
                                width: drawn.width,
                                height: math.max(1, viewport.height - gutter),
                                child: SingleChildScrollView(
                                  controller: viewportScroll,
                                  physics: controls
                                      ? const NeverScrollableScrollPhysics()
                                      : null,
                                  child: video(drawn),
                                ),
                              ),
                            ),
                          ),
                        ),
                      );
                    }
                    return Scrollbar(
                      key: const ValueKey('observe-scrollbar'),
                      controller: viewportScroll,
                      thumbVisibility: size.height > viewport.height,
                      interactive: true,
                      child: SingleChildScrollView(
                        controller: viewportScroll,
                        physics: controls
                            ? const NeverScrollableScrollPhysics()
                            : null,
                        child: video(size),
                      ),
                    );
                  },
                ),
              ),
            ),
            // Thin edge targets stay available even while all chrome is hidden.
            for (final top in [true, false])
              if (top || mode == 'observe')
                Positioned(
                  top: top ? 0 : null,
                  bottom: top ? null : 0,
                  left: 0,
                  right: 0,
                  height: 5,
                  child: MouseRegion(
                    key: ValueKey(
                      top ? 'observe-top-edge' : 'observe-bottom-edge',
                    ),
                    cursor: SystemMouseCursors.basic,
                    onEnter: (_) => revealChrome(),
                    child: GestureDetector(
                      onTap: revealChrome,
                      child: const ColoredBox(color: Color(0x183b82f6)),
                    ),
                  ),
                ),
            chrome(
              alignment: Alignment.topCenter,
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 4,
                      vertical: 5,
                    ),
                    child: Row(
                      children: [
                        const Icon(Icons.desktop_windows_outlined, size: 18),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            widget.name,
                            style: const TextStyle(fontWeight: FontWeight.w600),
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                        SegmentedButton<String>(
                          style: const ButtonStyle(
                            visualDensity: VisualDensity.compact,
                          ),
                          segments: const [
                            ButtonSegment(
                              value: 'observe',
                              label: Text('Observe'),
                              icon: Icon(Icons.visibility_outlined, size: 18),
                            ),
                            ButtonSegment(
                              value: 'control',
                              label: Text('Control'),
                              icon: Icon(Icons.mouse_outlined, size: 18),
                            ),
                          ],
                          selected: {mode == 'observe' ? 'observe' : 'control'},
                          onSelectionChanged:
                              canSwitchMode && requestedMode == null
                              ? (v) => switchMode(v.first)
                              : null,
                        ),
                        PopupMenuButton<String>(
                          tooltip: 'Student and control actions',
                          enabled: widget.agent.connected,
                          onSelected: (action) {
                            if (action == 'screenshot') {
                              saveStudentScreenshot();
                            } else if (action == 'message') {
                              messageStudent();
                            } else if (action == 'file') {
                              sendFileToStudent();
                            } else {
                              switchMode(action);
                            }
                            revealChrome();
                          },
                          itemBuilder: (_) => [
                            PopupMenuItem(
                              value: 'control',
                              enabled: canSwitchMode && requestedMode == null,
                              child: Text(
                                'Both teacher and student can control',
                              ),
                            ),
                            PopupMenuItem(
                              value: 'exclusive',
                              enabled: canSwitchMode && requestedMode == null,
                              child: Text(
                                'Teacher only · Lock student input (Windows)',
                              ),
                            ),
                            const PopupMenuDivider(),
                            PopupMenuItem(
                              value: 'screenshot',
                              enabled: live,
                              child: const ListTile(
                                dense: true,
                                leading: Icon(Icons.photo_camera_outlined),
                                title: Text('Save screenshot to Desktop'),
                              ),
                            ),
                            const PopupMenuItem(
                              value: 'message',
                              child: ListTile(
                                dense: true,
                                leading: Icon(Icons.chat_outlined),
                                title: Text('Send a message'),
                              ),
                            ),
                            const PopupMenuItem(
                              value: 'file',
                              child: ListTile(
                                dense: true,
                                leading: Icon(Icons.attach_file),
                                title: Text('Send a file'),
                              ),
                            ),
                          ],
                          icon: const Icon(Icons.tune, size: 20),
                        ),
                        const SizedBox(width: 4),
                        SegmentedButton<String>(
                          style: const ButtonStyle(
                            visualDensity: VisualDensity.compact,
                          ),
                          segments: const [
                            ButtonSegment(
                              value: 'stretch',
                              label: Text('Stretch'),
                              tooltip: 'Cover window and remove side bars',
                              icon: Icon(Icons.width_full, size: 18),
                            ),
                            ButtonSegment(
                              value: 'fill',
                              label: Text('Fill'),
                              tooltip: 'Fill window · Stretch image',
                              icon: Icon(Icons.fit_screen, size: 18),
                            ),
                          ],
                          selected: {displayMode},
                          onSelectionChanged: (v) {
                            if (viewportScroll.hasClients) {
                              viewportScroll.jumpTo(0);
                            }
                            setState(() => displayMode = v.first);
                            revealChrome();
                          },
                        ),
                        IconButton(
                          style: IconButton.styleFrom(
                            minimumSize: const Size(32, 32),
                            padding: const EdgeInsets.all(4),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          tooltip: fullScreen
                              ? 'Exit full screen'
                              : 'Full screen',
                          icon: Icon(
                            fullScreen
                                ? Icons.fullscreen_exit
                                : Icons.fullscreen,
                            size: 20,
                          ),
                          onPressed: toggleFullScreen,
                        ),
                        IconButton(
                          style: IconButton.styleFrom(
                            minimumSize: const Size(32, 32),
                            padding: const EdgeInsets.all(4),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          tooltip: 'Browse and collect files',
                          icon: const Icon(
                            Icons.folder_copy_outlined,
                            size: 19,
                          ),
                          onPressed: () {
                            widget.agent.request(
                              'files_window',
                              selected: [widget.studentId],
                            );
                            revealChrome();
                          },
                        ),
                        IconButton(
                          style: IconButton.styleFrom(
                            minimumSize: const Size(32, 32),
                            padding: const EdgeInsets.all(4),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          tooltip: 'Open windows, browser tabs and downloads',
                          icon: const Icon(Icons.visibility_outlined, size: 19),
                          onPressed: () {
                            widget.agent.request(
                              'activity_window',
                              selected: [widget.studentId],
                            );
                            revealChrome();
                          },
                        ),
                        IconButton(
                          style: IconButton.styleFrom(
                            minimumSize: const Size(32, 32),
                            padding: const EdgeInsets.all(4),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          key: const ValueKey('observe-stats-toggle'),
                          tooltip: showStats ? 'Hide stats' : 'Show stats',
                          isSelected: showStats,
                          icon: const Icon(Icons.speed_outlined, size: 19),
                          selectedIcon: const Icon(Icons.speed, size: 19),
                          onPressed: () {
                            setState(() => showStats = !showStats);
                            revealChrome();
                          },
                        ),
                        IconButton(
                          style: IconButton.styleFrom(
                            minimumSize: const Size(32, 32),
                            padding: const EdgeInsets.all(4),
                            tapTargetSize: MaterialTapTargetSize.shrinkWrap,
                          ),
                          tooltip: autoHide
                              ? 'Keep toolbar visible'
                              : 'Auto-hide toolbar',
                          isSelected: !autoHide,
                          icon: const Icon(Icons.push_pin_outlined, size: 18),
                          selectedIcon: const Icon(Icons.push_pin, size: 18),
                          onPressed: () {
                            setState(() => autoHide = !autoHide);
                            revealChrome();
                          },
                        ),
                      ],
                    ),
                  ),
                  if (error != null && error.isNotEmpty)
                    Container(
                      width: double.infinity,
                      color: Colors.orange.shade50,
                      padding: const EdgeInsets.all(8),
                      child: Text(error),
                    ),
                ],
              ),
            ),
            if (mode == 'observe')
              chrome(
                alignment: Alignment.bottomCenter,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 5,
                  ),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          modeError != null
                              ? modeError!
                              : reconnecting
                              ? 'Reconnecting to student…'
                              : requestedMode != null
                              ? 'Changing input mode…'
                              : mode == 'exclusive'
                              ? 'Teacher control · Student input locked'
                              : mode == 'control'
                              ? 'Shared control · Student input stays enabled'
                              : 'Observe · Read only',
                          style: const TextStyle(fontSize: 12),
                        ),
                      ),
                      if (live && sessionCodec != null) ...[
                        Text(
                          sessionCodec!,
                          key: const ValueKey('observe-footer-codec'),
                          style: const TextStyle(
                            fontSize: 12,
                            fontWeight: FontWeight.w600,
                            color: Colors.blueGrey,
                          ),
                        ),
                        const Text(
                          '  ·  ',
                          style: TextStyle(
                            fontSize: 12,
                            color: Colors.blueGrey,
                          ),
                        ),
                      ],
                      Text(
                        live
                            ? '${stats['width']} × ${stats['height']}'
                            : '${view['phase'] ?? 'Connecting'}',
                        style: const TextStyle(
                          fontSize: 12,
                          color: Colors.blueGrey,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            if (showStats)
              AnimatedPositioned(
                duration: const Duration(milliseconds: 180),
                top: chromeVisible ? 64 : 12,
                right: 12,
                child: ObserveStatsPanel(
                  key: const ValueKey('observe-stats-panel'),
                  data: ObserveStatsData(
                    mode: mode,
                    live: live,
                    native: stats,
                    metrics: Map<String, dynamic>.from(
                      view['metrics'] as Map? ?? {},
                    ),
                    fps: previewFps,
                  ),
                ),
              ),
            Positioned(
              right: 16,
              bottom: chromeVisible && mode == 'observe' ? 58 : 12,
              child: ClassroomToolsOverlay(
                agent: widget.agent,
                activity: toolActivity,
                preparingFile: preparingFile,
                onOpenReplies: () => showRepliesDialog(context, toolActivity),
              ),
            ),
            if (draggingFiles)
              Positioned.fill(
                child: IgnorePointer(
                  child: Container(
                    decoration: BoxDecoration(
                      color: const Color(0x661e6bd6),
                      border: Border.all(
                        color: const Color(0xff80baff),
                        width: 3,
                      ),
                    ),
                    alignment: Alignment.center,
                    child: const Card(
                      child: Padding(
                        padding: EdgeInsets.all(20),
                        child: Text(
                          'Drop to send to Desktop\nFolders will be zipped',
                          textAlign: TextAlign.center,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

/// One geometry for drawing and hit testing, including stretched and scrolled views.
Size observeImageSize(Size viewport, Size source, {required bool stretch}) {
  if (stretch) {
    // Fill intentionally stretches the decoded texture to every logical pixel.
    // Width mode below remains aspect preserving and can exceed the viewport.
    return Size(viewport.width, viewport.height);
  }
  // Native stats are zero before the first copied frame. Keep a valid texture
  // layout while connecting so the first frame can actually be painted.
  final ratio =
      source.width > 0 &&
          source.height > 0 &&
          source.width.isFinite &&
          source.height.isFinite
      ? source.height / source.width
      : 9 / 16;
  return Size(viewport.width, viewport.width * ratio);
}

Offset observePosition(Offset local, Size image, {Size? source, BoxFit? fit}) {
  if (fit == BoxFit.cover &&
      source != null &&
      source.width > 0 &&
      source.height > 0) {
    final scale = math.max(
      image.width / source.width,
      image.height / source.height,
    );
    final drawn = Size(source.width * scale, source.height * scale);
    final offset = Offset(
      (drawn.width - image.width) / 2,
      (drawn.height - image.height) / 2,
    );
    return Offset(
      ((local.dx + offset.dx) / drawn.width).clamp(0.0, 1.0),
      ((local.dy + offset.dy) / drawn.height).clamp(0.0, 1.0),
    );
  }
  return Offset(
    (local.dx / image.width).clamp(0.0, 1.0),
    (local.dy / image.height).clamp(0.0, 1.0),
  );
}
