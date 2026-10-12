import 'dart:io';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/remote_terminal.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm/xterm.dart';

class TerminalAgent extends Agent {
  TerminalAgent() : super([]);
  @override
  bool get connected => true;
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async => {
    'ok': false,
    'error': 'No network needed for keyboard regression',
  };
}

void main() {
  testWidgets('Windows terminal forwards printable keys and Enter once', (
    tester,
  ) async {
    if (!Platform.isWindows) return;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('window_manager'),
          (_) async => null,
        );
    final agent = TerminalAgent();
    final inputs = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: RemoteTerminalWindow(
          agent: agent,
          studentId: 'pc',
          name: 'Student',
        ),
      ),
    );
    await tester.pump();
    final dynamic state = tester.state(find.byType(RemoteTerminalWindow));
    state.setState(() {
      state.connected = true;
      state.error = null;
      (state.terminal as Terminal).onOutput = inputs.add;
    });
    await tester.pump();
    await tester.tap(find.byType(TerminalView));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyE, character: 'e');
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyE);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.space, character: ' ');
    await tester.sendKeyUpEvent(LogicalKeyboardKey.space);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.keyA, character: 'A');
    await tester.sendKeyUpEvent(LogicalKeyboardKey.keyA);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    expect(inputs.join(), 'e A\r');
    await tester.pumpWidget(const SizedBox());
    agent.dispose();
  });
}
