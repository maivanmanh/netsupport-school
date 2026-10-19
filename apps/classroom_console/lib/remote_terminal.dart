import 'icon_tints.dart';
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import 'package:xterm/xterm.dart';
import 'agent.dart';
import 'app_dialog.dart';

class RemoteTerminalWindow extends StatefulWidget {
  const RemoteTerminalWindow({
    super.key,
    required this.agent,
    required this.studentId,
    required this.name,
    this.now = DateTime.now,
  });
  final Agent agent;
  final String studentId, name;

  /// Clock used for the wait display; replaced in tests.
  final DateTime Function() now;
  @override
  State<RemoteTerminalWindow> createState() => _RemoteTerminalWindowState();
}

class _RemoteTerminalWindowState extends State<RemoteTerminalWindow>
    with WindowListener {
  Agent get agent => widget.agent;
  Terminal? terminal;
  final terminalController = TerminalController();
  final terminalFocus = FocusNode();
  String studentIp = '';
  bool exporting = false;
  Socket? stream;
  Timer? heartbeat, retry, ticker;
  late DateTime openedAt = widget.now();
  String? session, error;
  String status = 'Đang kết nối tới học viên…', os = 'windows';
  bool connected = false, starting = false, closing = false, ended = false;
  bool everConnected = false;
  int lastSequence = 0, queuedInput = 0;
  DateTime? disconnectedAt;
  final bytes = <int>[];
  ByteConversionSink? decoder;
  Future<void> inputWrites = Future.value();
  double fontPt = 12;
  String family = '', fallback = 'Consolas, Menlo, monospace';
  int scrollback = 5000;
  bool black = false, compatibility = true;
  bool processingOutput = false;

  @override
  void initState() {
    super.initState();
    agent.addListener(ready);
    windowManager.addListener(this);
    unawaited(windowManager.setPreventClose(true));
    // Re-render once a second while waiting so the Teacher sees elapsed time
    // instead of a frozen "connecting" label on a slow or unreachable Student.
    ticker = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted && !connected && !ended && !closing) setState(() {});
    });
    ready();
  }

  /// Status line that reflects how long we have been waiting and why.
  String get displayStatus {
    if (connected || ended) return status;
    if (!agent.connected) return 'Mất kết nối với chương trình giảng viên…';
    final online = agent.students.any(
      (s) => s['id'] == widget.studentId && s['online'] == true,
    );
    if (!online) {
      return everConnected
          ? 'Máy học viên đang offline · sẽ tự nối lại khi máy trở lại…'
          : 'Máy học viên đang offline. Hãy kiểm tra máy đã bật và kết nối mạng.';
    }
    final seconds = widget
        .now()
        .difference(disconnectedAt ?? openedAt)
        .inSeconds;
    if (seconds < 3) return status;
    if (seconds < 10) {
      return 'Máy học viên phản hồi chậm… (${seconds}s)';
    }
    return 'Vẫn đang chờ học viên (${seconds}s) · máy có thể đang bận hoặc mạng chậm';
  }

  /// Abandons the current session and opens a fresh one on the same Student.
  void restart() {
    final old = session;
    heartbeat?.cancel();
    retry?.cancel();
    stream?.destroy();
    stream = null;
    decoder?.close();
    bytes.clear();
    setState(() {
      error = null;
      session = null;
      ended = false;
      starting = false;
      connected = false;
      everConnected = false;
      lastSequence = 0;
      disconnectedAt = null;
      openedAt = widget.now();
      status = 'Đang kết nối tới học viên…';
    });
    if (old != null) {
      unawaited(agent.request('terminal_close', input: {'session': old}));
    }
    ready();
  }

  void ready() {
    if (!agent.connected || starting || session != null || ended || closing) {
      return;
    }
    starting = true;
    final caps = (agent.state['tools'] as Map?)?['capabilities'] as Map?;
    os = '${(caps?[widget.studentId] as Map?)?['os'] ?? 'windows'}';
    for (final row in (agent.state['students'] as List? ?? const [])) {
      if (row is Map && row['id'] == widget.studentId) {
        studentIp = '${row['ip'] ?? ''}';
        break;
      }
    }
    unawaited(
      windowManager.setTitle(
        'Remote Terminal • ${widget.name}${studentIp.isEmpty ? '' : ' · $studentIp'}',
      ),
    );
    final p = agent.preferences;
    unawaited(
      windowManager.setSize(
        Size(
          (p['terminal_window_width'] as num? ?? 1000).toDouble().clamp(
            640,
            3000,
          ),
          (p['terminal_window_height'] as num? ?? 650).toDouble().clamp(
            400,
            2000,
          ),
        ),
      ),
    );
    fontPt = (p['terminal_font_pt'] as num? ?? 12).toDouble().clamp(8, 30);
    family = '${p['terminal_font_$os'] ?? ''}';
    fallback = '${p['terminal_font_fallback'] ?? fallback}';
    scrollback = (p['terminal_scrollback'] as num? ?? 5000).toInt().clamp(
      100,
      50000,
    );
    black = p['terminal_black'] == true;
    compatibility = p['terminal_windows_compatibility'] != false;
    terminal = Terminal(
      maxLines: scrollback,
      platform: os == 'macos'
          ? TerminalTargetPlatform.macos
          : TerminalTargetPlatform.windows,
      onOutput: sendInput,
      onResize: (cols, rows, _, _) {
        if (!connected) return;
        final size = ByteData(4)
          ..setUint16(0, cols.clamp(2, 500))
          ..setUint16(2, rows.clamp(2, 300));
        frame(3, 0, size.buffer.asUint8List());
      },
    );
    decoder = utf8.decoder.startChunkedConversion(
      StringConversionSink.fromStringSink(_TerminalSink(terminal!)),
    );
    unawaited(start());
  }

  Future<void> start() async {
    final answer = await agent.request(
      'terminal_open',
      selected: [widget.studentId],
      input: {'compatibility': compatibility},
    );
    if (!mounted || closing) {
      if (answer['ok'] == true) {
        await agent.request(
          'terminal_close',
          input: {'session': (answer['result'] as Map)['session']},
        );
      }
      return;
    }
    if (answer['ok'] != true) {
      setState(() {
        error = Agent.failure(answer);
        status = 'Remote Terminal unavailable';
      });
      return;
    }
    session = '${(answer['result'] as Map)['session']}';
    await attach();
  }

  Future<void> attach() async {
    if (closing || ended || !mounted) return;
    try {
      final descriptor = jsonDecode(await agent.endpoint.readAsString()) as Map;
      final socket = await Socket.connect(
        InternetAddress.loopbackIPv4,
        descriptor['port'] as int,
        timeout: const Duration(seconds: 3),
      );
      if (!mounted || closing) {
        socket.destroy();
        return;
      }
      socket.setOption(SocketOption.tcpNoDelay, true);
      stream = socket;
      bytes.clear();
      final hello = utf8.encode(
        jsonEncode({
          'type': 'hello',
          'protocol': 1,
          'token': descriptor['token'],
          'ui_role': 'terminal',
          'session': session,
          'last_seq': lastSequence,
        }),
      );
      final prefix = ByteData(4)..setUint32(0, hello.length);
      socket.add(prefix.buffer.asUint8List());
      socket.add(hello);
      socket.listen(
        (chunk) {
          if (identical(stream, socket)) receive(chunk);
        },
        onDone: () {
          if (identical(stream, socket)) lost();
        },
        onError: (Object e) {
          if (!identical(stream, socket)) return;
          error = '$e';
          lost();
        },
        cancelOnError: true,
      );
      heartbeat?.cancel();
      heartbeat = Timer.periodic(
        const Duration(seconds: 2),
        (_) => frame(8, 0, []),
      );
    } catch (e) {
      error = '$e';
      lost();
    }
  }

  void receive(Uint8List chunk) {
    bytes.addAll(chunk);
    while (bytes.length >= 13) {
      final header = ByteData.sublistView(
        Uint8List.fromList(bytes.sublist(0, 13)),
      );
      final kind = header.getUint8(0),
          sequence = header.getUint64(1),
          length = header.getUint32(9);
      if (length > 16384 || kind < 1 || kind > 9) {
        error = 'Khung dữ liệu terminal không hợp lệ';
        ended = true;
        lost();
        return;
      }
      if (bytes.length < length + 13) break;
      final body = Uint8List.fromList(bytes.sublist(13, length + 13));
      bytes.removeRange(0, length + 13);
      if (kind == 1) {
        if (sequence > lastSequence) {
          if (sequence != lastSequence + 1) {
            error = 'Dữ liệu terminal bị đứt đoạn';
            ended = true;
            lost();
            return;
          }
          processingOutput = true;
          try {
            decoder?.add(body);
          } finally {
            processingOutput = false;
          }
          lastSequence = sequence;
        }
        frame(7, sequence, []);
      } else if (kind == 5) {
        final metadata = jsonDecode(utf8.decode(body)) as Map;
        final active =
            metadata['state'] == 'ready' && metadata['connected'] == true;
        if (active) {
          everConnected = true;
          disconnectedAt = null;
          if (!connected) {
            final size = ByteData(4)
              ..setUint16(0, terminal!.viewWidth.clamp(2, 500))
              ..setUint16(2, terminal!.viewHeight.clamp(2, 300));
            frame(3, 0, size.buffer.asUint8List());
          }
        } else {
          disconnectedAt ??= DateTime.now();
          if (DateTime.now().difference(disconnectedAt!).inSeconds >= 30) {
            ended = true;
            heartbeat?.cancel();
            unawaited(
              agent.request('terminal_close', input: {'session': session}),
            );
          }
        }
        if (mounted) {
          setState(() {
            connected = active;
            status = active
                ? '${metadata['shell']} · ${metadata['cwd']}'
                : ended
                ? 'Phiên đã kết thúc'
                : everConnected
                ? 'Đang kết nối lại tới học viên…'
                : 'Đang kết nối tới học viên…';
          });
        }
      } else if (kind == 6) {
        ended = true;
        connected = false;
        if (body.isNotEmpty) {
          error = utf8.decode(body, allowMalformed: true);
        }
        if (mounted) {
          setState(() {
            status = everConnected
                ? 'Shell đã thoát'
                : 'Không kết nối được tới học viên';
          });
        }
      }
    }
  }

  void lost() {
    heartbeat?.cancel();
    stream?.destroy();
    stream = null;
    if (closing || !mounted) return;
    disconnectedAt ??= DateTime.now();
    if (DateTime.now().difference(disconnectedAt!).inSeconds >= 30) {
      ended = true;
      unawaited(agent.request('terminal_close', input: {'session': session}));
    }
    setState(() {
      connected = false;
      status = ended
          ? 'Phiên đã kết thúc'
          : everConnected
          ? 'Đang kết nối lại tới học viên…'
          : 'Đang kết nối tới học viên…';
    });
    if (!ended) {
      retry?.cancel();
      retry = Timer(const Duration(milliseconds: 500), attach);
    }
  }

  void frame(int kind, int sequence, List<int> data) {
    final s = stream;
    if (s == null || closing) return;
    final head = ByteData(13)
      ..setUint8(0, kind)
      ..setUint64(1, sequence)
      ..setUint32(9, data.length);
    s.add(head.buffer.asUint8List());
    s.add(data);
  }

  void sendInput(String text) {
    final protocolReply = processingOutput;
    final socket = stream;
    if ((!connected && !protocolReply) || socket == null || closing || ended) {
      return;
    }
    final data = utf8.encode(text);
    if (queuedInput + data.length > 65536) {
      setState(() {
        error =
            'Hàng đợi nhập đã đầy. Hãy chờ rồi thử lại; nội dung dán lớn chưa được gửi.';
      });
      return;
    }
    queuedInput += data.length;
    inputWrites = inputWrites
        .then((_) async {
          if ((!connected && !protocolReply) || !identical(stream, socket)) {
            throw StateError('Chưa gửi được phím: phiên đã mất kết nối.');
          }
          for (var i = 0; i < data.length; i += 16384) {
            if (!identical(stream, socket)) {
              throw StateError('Chưa gửi được phím: kết nối vừa thay đổi.');
            }
            frame(2, 0, data.sublist(i, (i + 16384).clamp(0, data.length)));
            await socket.flush();
          }
        })
        .catchError((Object e) {
          if (mounted) {
            setState(() {
              error = '$e';
            });
          }
        })
        .whenComplete(() {
          queuedInput -= data.length;
        });
  }

  Future<void> settings() async {
    final font = TextEditingController(text: family),
        fall = TextEditingController(text: fallback),
        size = TextEditingController(text: '$fontPt'),
        lines = TextEditingController(text: '$scrollback');
    var nextBlack = black, nextCompatibility = compatibility;
    final saved = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setLocal) => AppDialog(
          icon: Icons.terminal,
          title: 'Cài đặt Terminal',
          subtitle: 'Áp dụng cho cửa sổ Terminal từ xa này.',
          width: 480,
          onClose: () => Navigator.pop(context, false),
          actions: [
            AppDialog.cancel(
              context,
              onPressed: () => Navigator.pop(context, false),
            ),
            ListenableBuilder(
              listenable: Listenable.merge([font, fall, size, lines]),
              builder: (context, _) {
                // Enabled only when something differs from the current settings.
                final changed =
                    font.text.trim() != family ||
                    fall.text != fallback ||
                    (double.tryParse(size.text) ?? fontPt) != fontPt ||
                    (int.tryParse(lines.text) ?? scrollback) != scrollback ||
                    nextBlack != black ||
                    nextCompatibility != compatibility;
                return AppDialog.primary(
                  onPressed: changed
                      ? () => Navigator.pop(context, true)
                      : null,
                  icon: Icons.check,
                  label: 'Lưu',
                );
              },
            ),
          ],
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TextField(
                controller: font,
                decoration: InputDecoration(
                  labelText: 'Phông chữ',
                  hintText: os == 'windows' ? 'Consolas' : 'Menlo',
                ),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: fall,
                decoration: const InputDecoration(
                  labelText: 'Phông chữ dự phòng (cách nhau bằng dấu phẩy)',
                ),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: size,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'Cỡ chữ (pt, 8–30)',
                ),
              ),
              const SizedBox(height: 12),
              TextField(
                controller: lines,
                keyboardType: TextInputType.number,
                decoration: const InputDecoration(
                  labelText: 'Số dòng cuộn lại (phiên sau, 100–50000)',
                ),
              ),
              const SizedBox(height: 8),
              if (os == 'windows')
                SwitchListTile(
                  contentPadding: EdgeInsets.zero,
                  value: nextCompatibility,
                  onChanged: (v) => setLocal(() => nextCompatibility = v),
                  title: const Text('Lệnh kiểu Unix cho CMD'),
                  subtitle: const Text(
                    'Áp dụng cho phiên sau. Lệnh Windows có sẵn được ưu tiên.',
                  ),
                ),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                value: nextBlack,
                onChanged: (v) => setLocal(() => nextBlack = v),
                title: const Text('Chữ trắng nền đen'),
              ),
            ],
          ),
        ),
      ),
    );
    if (saved == true && mounted) {
      setState(() {
        family = font.text.trim();
        fallback = fall.text;
        fontPt = (double.tryParse(size.text) ?? 12).clamp(8, 30);
        scrollback = (int.tryParse(lines.text) ?? 5000).clamp(100, 50000);
        black = nextBlack;
        compatibility = nextCompatibility;
      });
      agent.savePreferences({
        'terminal_font_$os': family,
        'terminal_font_fallback': fallback,
        'terminal_font_pt': fontPt,
        'terminal_scrollback': scrollback,
        'terminal_black': black,
        'terminal_windows_compatibility': compatibility,
      });
    }
    disposeAfterClose(font);
    disposeAfterClose(fall);
    disposeAfterClose(size);
    disposeAfterClose(lines);
  }

  Future<void> copySelection() async {
    final selection = terminalController.selection;
    if (selection == null || terminal == null) return;
    await Clipboard.setData(
      ClipboardData(text: terminal!.buffer.getText(selection)),
    );
  }

  Future<void> pasteClipboard() async {
    if (!connected || closing || ended) return;
    final text = (await Clipboard.getData(Clipboard.kTextPlain))?.text;
    if (text == null || !mounted || !connected) return;
    // LF outside bracketed paste is Ctrl+J in editors such as nano (justify),
    // rather than a physical Enter. Preserve every line and whitespace; only
    // normalize newline transport when the application has no paste protocol.
    final normalized = text.replaceAll('\r\n', '\n').replaceAll('\r', '\n');
    final value = terminal!.bracketedPasteMode
        ? normalized
        : normalized.replaceAll('\n', '\r');
    terminalController.clearSelection();
    terminal!.paste(value);
    terminalFocus.requestFocus();
  }

  KeyEventResult terminalKey(FocusNode node, KeyEvent event) {
    // xterm's hardware listener must not insert a character again on key-up.
    if (Platform.isWindows && event is KeyUpEvent) {
      return KeyEventResult.handled;
    }
    final keyboard = HardwareKeyboard.instance;
    final copyModifier = Platform.isMacOS
        ? keyboard.isMetaPressed
        : keyboard.isControlPressed;
    if (event is! KeyDownEvent || !copyModifier) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.keyC &&
        terminalController.selection != null) {
      unawaited(copySelection());
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.keyV) {
      unawaited(pasteClipboard());
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  Future<void> terminalMenu(TapDownDetails details) async {
    final overlay = Overlay.of(context).context.findRenderObject() as RenderBox;
    final selected = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(
        details.globalPosition & const Size(1, 1),
        Offset.zero & overlay.size,
      ),
      items: [
        PopupMenuItem(
          value: 'copy',
          enabled: terminalController.selection != null,
          child: Text('Sao chép    ${Platform.isMacOS ? '⌘C' : 'Ctrl+C'}'),
        ),
        PopupMenuItem(
          value: 'paste',
          enabled: connected,
          child: Text('Dán    ${Platform.isMacOS ? '⌘V' : 'Ctrl+V'}'),
        ),
      ],
    );
    if (selected == 'copy') await copySelection();
    if (selected == 'paste') await pasteClipboard();
    terminalFocus.requestFocus();
  }

  Future<void> downloadOutput() async {
    if (terminal == null || exporting) return;
    // Include the complete available scrollback, not just the visible viewport.
    final text =
        terminal!.mainBuffer.getText() +
        (terminal!.isUsingAltBuffer
            ? '\n\n--- Màn hình đang hoạt động ---\n${terminal!.altBuffer.getText()}'
            : '');
    setState(() => exporting = true);
    try {
      final reply = await agent.request('terminal_desktop');
      if (reply['ok'] != true) throw StateError(Agent.failure(reply));
      final desktop = '${(reply['result'] as Map)['path']}';
      final name = widget.name.replaceAll(
        RegExp(r'[<>:"/\\|?*\x00-\x1f]'),
        '_',
      );
      final stamp = DateTime.now()
          .toIso8601String()
          .replaceAll(':', '-')
          .replaceAll('.', '-');
      final file = File(
        '$desktop${Platform.pathSeparator}${name}_${studentIp}_Terminal_$stamp.txt',
      );
      await file.writeAsString(text, encoding: utf8, flush: true);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          SnackBar(
            content: Text('Đã lưu vào Desktop: ${file.uri.pathSegments.last}'),
          ),
        );
      }
    } catch (e) {
      if (mounted) {
        setState(() => error = 'Không lưu được nội dung terminal: $e');
      }
    } finally {
      if (mounted) setState(() => exporting = false);
    }
  }

  @override
  Future<void> onWindowClose() async {
    if (closing) return;
    frame(4, 0, []);
    closing = true;
    if (session != null) {
      await agent.request('terminal_close', input: {'session': session});
    }
    final bounds = await windowManager.getBounds();
    agent.savePreferences({
      'terminal_window_width': bounds.width,
      'terminal_window_height': bounds.height,
    });
    await agent.finishApplication();
  }

  @override
  void dispose() {
    closing = true;
    agent.removeListener(ready);
    windowManager.removeListener(this);
    heartbeat?.cancel();
    retry?.cancel();
    ticker?.cancel();
    stream?.destroy();
    decoder?.close();
    terminalController.dispose();
    terminalFocus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    backgroundColor: const Color(0xff1e1e1e),
    body: Column(
      children: [
        Container(
          height: 40,
          decoration: const BoxDecoration(
            color: Color(0xff26323c),
            border: Border(bottom: BorderSide(color: Color(0xff34444f))),
          ),
          padding: const EdgeInsets.only(left: 10, right: 4),
          child: Row(
            children: [
              TIcon(
                connected ? Icons.circle : Icons.sync,
                size: 8,
                color: connected ? Colors.greenAccent : Colors.amber,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  displayStatus,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: Colors.white70, fontSize: 12),
                ),
              ),
              IconButton(
                onPressed: settings,
                tooltip: 'Cài đặt Terminal',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const TIcon(Icons.tune),
              ),
              IconButton(
                onPressed: terminal != null && !exporting
                    ? downloadOutput
                    : null,
                tooltip: 'Lưu toàn bộ nội dung hiện có vào Desktop',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const TIcon(Icons.download_outlined),
              ),
              IconButton(
                onPressed: connected ? () => sendInput('\x03') : null,
                tooltip: 'Gửi Ctrl+C',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const TIcon(Icons.stop_circle_outlined),
              ),
            ],
          ),
        ),
        if (error != null)
          MaterialBanner(
            content: Text(error!),
            actions: [
              if (session == null || ended)
                TextButton(
                  onPressed: restart,
                  child: Text(ended ? 'Mở phiên mới' : 'Thử lại'),
                ),
              TextButton(
                onPressed: () => setState(() => error = null),
                child: const Text('Bỏ qua'),
              ),
            ],
          ),
        Expanded(
          child: terminal == null
              ? const Center(child: CircularProgressIndicator())
              : RepaintBoundary(
                  // xterm 4 paints selection in local canvas coordinates. Give
                  // the viewport its own origin so toolbar/window offsets cannot
                  // shift highlights away from their text.
                  child: TerminalView(
                    terminal!,
                    controller: terminalController,
                    focusNode: terminalFocus,
                    onKeyEvent: terminalKey,
                    onSecondaryTapDown: (details, _) =>
                        unawaited(terminalMenu(details)),
                    textScaler: TextScaler.noScaling,
                    autofocus: true,
                    // Windows delivered Enter through key events but printable
                    // keys were lost by the text-input bridge. xterm provides a
                    // desktop hardware path that forwards KeyEvent.character.
                    hardwareKeyboardOnly: Platform.isWindows,
                    readOnly: !connected,
                    theme: black
                        ? TerminalThemes.whiteOnBlack
                        : TerminalThemes.defaultTheme,
                    textStyle: TerminalStyle(
                      fontFamily: family.isEmpty
                          ? (os == 'windows' ? 'Consolas' : 'Menlo')
                          : family,
                      fontFamilyFallback: fallback
                          .split(',')
                          .map((s) => s.trim())
                          .where((s) => s.isNotEmpty)
                          .toList(),
                      fontSize: fontPt * 4 / 3,
                    ),
                  ),
                ),
        ),
      ],
    ),
  );
}

class _TerminalSink implements StringSink {
  _TerminalSink(this.terminal);
  final Terminal terminal;
  @override
  void write(Object? object) => terminal.write('$object');
  @override
  void writeAll(Iterable objects, [String separator = '']) =>
      write(objects.join(separator));
  @override
  void writeln([Object? object = '']) => write('$object\n');
  @override
  void writeCharCode(int charCode) => write(String.fromCharCode(charCode));
}
