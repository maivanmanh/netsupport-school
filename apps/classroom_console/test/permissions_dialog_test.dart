import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/permissions_dialog.dart';
import 'support/fake_store.dart';

class PermissionAgent extends Agent with FakeStoreAgent {
  PermissionAgent([FakeStore? shared])
    : store = shared ?? FakeStore(),
      super([
        '--ipc-file',
        '/fixture/test-endpoint.json',
        '--control-port',
        '48900',
      ]);
  @override
  final FakeStore store;
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
          if (call.method == 'markOnboardingSeen') {
            status = {...status, 'onboardingSeen': true};
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
    await tester.tap(find.text('Tiếp tục'));
    await tester.pumpAndSettle();
    agent.dispose();
  }

  bool seen(PermissionAgent agent) =>
      agent.store.data['ui']?['mac_permissions_onboarding_seen'] == true;
  Future<PermissionAgent> autoOpen(
    WidgetTester tester, [
    FakeStore? shared,
  ]) async {
    final agent = PermissionAgent(shared)..state = {'phase': 'idle'};
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => maybeShowMacPermissions(context, agent),
              child: const Text('Auto'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Auto'));
    await tester.pumpAndSettle();
    return agent;
  }

  testWidgets('automatic checklist shows only once when both grants missing', (
    tester,
  ) async {
    final agent = await autoOpen(tester);
    expect(find.text('Thiết lập quyền macOS'), findsOneWidget);
    expect(seen(agent), isTrue);
    expect(calls, isNot(contains('markOnboardingSeen')));
    await tester.tap(find.text('Tiếp tục'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Auto'));
    await tester.pumpAndSettle();
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    agent.dispose();
  });
  testWidgets('partial grant skips startup checklist and remembers it', (
    tester,
  ) async {
    status = {...status, 'screen': true};
    final agent = await autoOpen(tester);
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    expect(seen(agent), isTrue);
    agent.dispose();
  });
  testWidgets('unknown helper does not show or remember automatic checklist', (
    tester,
  ) async {
    status = {...status, 'helperScreen': null};
    final agent = await autoOpen(tester);
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    expect(seen(agent), isFalse);
    agent.dispose();
  });
  testWidgets('revoked grants do not reopen a previously seen checklist', (
    tester,
  ) async {
    status = {...status, 'onboardingSeen': true};
    final agent = await autoOpen(tester);
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    // The marker kept by older builds in macOS preferences moves to the database.
    expect(seen(agent), isTrue);
    expect(calls, isNot(contains('markOnboardingSeen')));
    agent.dispose();
  });
  testWidgets('seen marker in the database suppresses the checklist', (
    tester,
  ) async {
    final shared = FakeStore()
      ..apply('ui', {'mac_permissions_onboarding_seen': true});
    final agent = await autoOpen(tester, shared);
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    agent.dispose();
  });
  testWidgets('polls grants without requesting or resetting privacy', (
    tester,
  ) async {
    final agent = await open(tester);
    expect(find.text('Chưa cấp'), findsNWidgets(2));
    status = {...status, 'screen': true, 'helperScreen': true};
    await tester.pump(const Duration(seconds: 2));
    await tester.pump();
    expect(find.text('Đã cấp'), findsNWidgets(2));
    expect(find.textContaining('quyền Screen Recording mới'), findsOneWidget);
    expect(calls.every((call) => call == 'status'), isTrue);
    await close(tester, agent);
  });
  testWidgets('LAN is verified only after both identities connect', (
    tester,
  ) async {
    final agent = await open(tester);
    await tester.ensureVisible(find.text('Yêu cầu / Thử truy cập LAN'));
    await tester.tap(find.text('Yêu cầu / Thử truy cập LAN'));
    await tester.pumpAndSettle();
    expect(find.text('Đã kết nối (lần thử gần nhất)'), findsNothing);
    network = {
      'console': {'state': 'connected'},
      'helper': {'state': 'connected'},
    };
    await tester.pump(const Duration(seconds: 10));
    await tester.pumpAndSettle();
    expect(find.text('Đã kết nối (lần thử gần nhất)'), findsOneWidget);
    // Editing the endpoint invalidates the previous green result.
    await tester.enterText(find.byType(TextField).first, '192.168.50.106');
    await tester.pump();
    expect(find.text('Đã kết nối (lần thử gần nhất)'), findsNothing);
    await close(tester, agent);
  });
  testWidgets(
    'Continue works with missing grants; restart disabled during sharing',
    (tester) async {
      final agent = await open(tester, phase: 'sharing');
      final restart = tester.widget<OutlinedButton>(
        find.widgetWithText(OutlinedButton, 'Khởi động lại Teacher'),
      );
      expect(restart.onPressed, isNull);
      await close(tester, agent);
      expect(agent.requested, isNull);
      expect(find.text('Thiết lập quyền macOS'), findsNothing);
    },
  );
  testWidgets('idle restart invokes existing Teacher lifecycle', (
    tester,
  ) async {
    final agent = await open(tester);
    await tester.tap(find.text('Khởi động lại Teacher'));
    await tester.pumpAndSettle();
    expect(agent.requested, 'restart');
    expect(restartArguments, agent.args);
    expect(find.text('Thiết lập quyền macOS'), findsNothing);
    agent.dispose();
  });
}
