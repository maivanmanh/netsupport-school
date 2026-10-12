import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:window_manager/window_manager.dart';
import 'package:xterm/xterm.dart';
import 'agent.dart';

class RemoteTerminalWindow extends StatefulWidget {
  const RemoteTerminalWindow({
    super.key,
    required this.agent,
    required this.studentId,
    required this.name,
  });
  final Agent agent;
  final String studentId, name;
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
  Timer? heartbeat, retry;
  String? session, error;
  String status = 'Connecting to student…', os = 'windows';
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
        error = 'Invalid terminal frame';
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
            error = 'Terminal output sequence gap';
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
          if (DateTime.now().difference(disconnectedAt!).inSeconds >= 15) {
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
                ? 'Session ended'
                : everConnected
                ? 'Reconnecting to student…'
                : 'Connecting to student…';
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
            status = 'Shell exited';
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
    if (DateTime.now().difference(disconnectedAt!).inSeconds >= 15) {
      ended = true;
      unawaited(agent.request('terminal_close', input: {'session': session}));
    }
    setState(() {
      connected = false;
      status = ended
          ? 'Session ended'
          : everConnected
          ? 'Reconnecting to student…'
          : 'Connecting to student…';
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
        error = 'Input queue full. Wait and retry; large paste was not sent.';
      });
      return;
    }
    queuedInput += data.length;
    inputWrites = inputWrites
        .then((_) async {
          if ((!connected && !protocolReply) || !identical(stream, socket)) {
            throw StateError('Input not sent: session disconnected');
          }
          for (var i = 0; i < data.length; i += 16384) {
            if (!identical(stream, socket)) {
              throw StateError('Input not sent: connection changed');
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
        builder: (context, setLocal) => AlertDialog(
          title: const Text('Terminal settings'),
          content: SizedBox(
            width: 420,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: font,
                  decoration: InputDecoration(
                    labelText: 'Font family',
                    hintText: os == 'windows' ? 'Consolas' : 'Menlo',
                  ),
                ),
                TextField(
                  controller: fall,
                  decoration: const InputDecoration(
                    labelText: 'Fallback fonts (comma separated)',
                  ),
                ),
                TextField(
                  controller: size,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Font size (pt, 8–30)',
                  ),
                ),
                TextField(
                  controller: lines,
                  keyboardType: TextInputType.number,
                  decoration: const InputDecoration(
                    labelText: 'Scrollback lines (next session, 100–50000)',
                  ),
                ),
                if (os == 'windows')
                  SwitchListTile(
                    value: nextCompatibility,
                    onChanged: (v) => setLocal(() => nextCompatibility = v),
                    title: const Text('Unix-like commands for CMD'),
                    subtitle: const Text(
                      'Applies to the next session. Existing Windows commands take priority.',
                    ),
                  ),
                SwitchListTile(
                  value: nextBlack,
                  onChanged: (v) => setLocal(() => nextBlack = v),
                  title: const Text('White on black'),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Save'),
            ),
          ],
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
    font.dispose();
    fall.dispose();
    size.dispose();
    lines.dispose();
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
    if (Platform.isWindows && event is KeyUpEvent)
      return KeyEventResult.handled;
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
          child: Text('Copy    ${Platform.isMacOS ? '⌘C' : 'Ctrl+C'}'),
        ),
        PopupMenuItem(
          value: 'paste',
          enabled: connected,
          child: Text('Paste    ${Platform.isMacOS ? '⌘V' : 'Ctrl+V'}'),
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
            ? '\n\n--- Active screen ---\n${terminal!.altBuffer.getText()}'
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
            content: Text('Saved to Desktop: ${file.uri.pathSegments.last}'),
          ),
        );
      }
    } catch (e) {
      if (mounted) setState(() => error = 'Could not save terminal output: $e');
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
          height: 36,
          color: const Color(0xff26323c),
          padding: const EdgeInsets.only(left: 10, right: 4),
          child: Row(
            children: [
              Icon(
                connected ? Icons.circle : Icons.sync,
                size: 8,
                color: connected ? Colors.greenAccent : Colors.amber,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  status,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: Colors.white70, fontSize: 11),
                ),
              ),
              IconButton(
                onPressed: settings,
                tooltip: 'Terminal settings',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.tune),
              ),
              IconButton(
                onPressed: terminal != null && !exporting
                    ? downloadOutput
                    : null,
                tooltip: 'Save all available output to Desktop',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.download_outlined),
              ),
              IconButton(
                onPressed: connected ? () => sendInput('\x03') : null,
                tooltip: 'Send Ctrl+C',
                iconSize: 18,
                visualDensity: VisualDensity.compact,
                icon: const Icon(Icons.stop_circle_outlined),
              ),
            ],
          ),
        ),
        if (error != null)
          MaterialBanner(
            content: Text(error!),
            actions: [
              if (session == null)
                TextButton(
                  onPressed: () {
                    setState(() {
                      error = null;
                      starting = false;
                    });
                    ready();
                  },
                  child: const Text('Retry'),
                ),
              TextButton(
                onPressed: () => setState(() => error = null),
                child: const Text('Dismiss'),
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
