import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/permissions_dialog.dart';

class PermissionAgent extends Agent {
  PermissionAgent()
    : super([
        '--ipc-file',
        '/fixture/test-endpoint.json',
        '--control-port',
        '48900',
      ]);
  @override
  bool get connected => true;
  String? requested;
  @override
  Future<void> commandChecked(
    String action,
    Iterable<String> ids,
    Map<String, dynamic> input,
  ) async {
    requested = action;
  }
}

void main() {
  const channel = MethodChannel('classroom/permissions');
  TestWidgetsFlutterBinding.ensureInitialized();
  Map<String, dynamic> status = {};
  Map<String, dynamic> network = {};
  final calls = <String>[];
  Object? restartArguments;
  setUp(() {
    status = {
      'screen': false,
      'helperScreen': false,
      'appPath': '/current/Classroom Console.app',
      'helperPath': '/current/Classroom Agent.app',
    };
    network = {
      'console': {'state': 'connected'},
      'helper': {'state': 'unverified', 'detail': 'No route'},
    };
    calls.clear();
    restartArguments = null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, (call) async {
          calls.add(call.method);
          if (call.method == 'restartConsole') {
            restartArguments = call.arguments;
          }
          if (call.method == 'status') return status;
          if (call.method == 'probeNetwork') return network;
          return true;
        });
  });
  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(channel, null);
  });
  Future<PermissionAgent> open(
    WidgetTester tester, {
    String phase = 'idle',
  }) async {
    final agent = PermissionAgent()
      ..state = {
        'phase': phase,
        'students': [
          {'ip': '192.168.50.94'},
        ],
      };
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => showMacPermissions(context, agent),
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    return agent;
  }

  Future<void> close(WidgetTester tester, PermissionAgent agent) async {
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();
    agent.dispose();
  }

  testWidgets('polls grants without requesting or resetting privacy', (
    tester,
  ) async {
    final agent = await open(tester);
    expect(find.text('Not granted'), findsNWidgets(2));
    status = {...status, 'screen': true, 'helperScreen': true};
    await tester.pump(const Duration(seconds: 2));
    await tester.pump();
    expect(find.text('Granted'), findsNWidgets(2));
    expect(find.textContaining('New Screen Recording grant'), findsOneWidget);
    expect(calls.every((call) => call == 'status'), isTrue);
    await close(tester, agent);
  });
  testWidgets('LAN is verified only after both identities connect', (
    tester,
  ) async {
    final agent = await open(tester);
    await tester.ensureVisible(find.text('Request / Test LAN access'));
    await tester.tap(find.text('Request / Test LAN access'));
    await tester.pumpAndSettle();
    expect(find.text('Connected (last test)'), findsNothing);
    network = {
      'console': {'state': 'connected'},
      'helper': {'state': 'connected'},
    };
    await tester.pump(const Duration(seconds: 10));
    await tester.pumpAndSettle();
    expect(find.text('Connected (last test)'), findsOneWidget);
    // Editing the endpoint invalidates the previous green result.
    await tester.enterText(find.byType(TextField).first, '192.168.50.106');
    await tester.pump();
    expect(find.text('Connected (last test)'), findsNothing);
    await close(tester, agent);
  });
  testWidgets(
    'Continue works with missing grants; restart disabled during sharing',
    (tester) async {
      final agent = await open(tester, phase: 'sharing');
      final restart = tester.widget<TextButton>(
        find.widgetWithText(TextButton, 'Restart Teacher'),
      );
      expect(restart.onPressed, isNull);
      await close(tester, agent);
      expect(agent.requested, isNull);
      expect(find.text('Set up macOS permissions'), findsNothing);
    },
  );
  testWidgets('idle restart invokes existing Teacher lifecycle', (
    tester,
  ) async {
    final agent = await open(tester);
    await tester.tap(find.text('Restart Teacher'));
    await tester.pumpAndSettle();
    expect(agent.requested, 'restart');
    expect(restartArguments, agent.args);
    expect(find.text('Set up macOS permissions'), findsNothing);
    agent.dispose();
  });
}
