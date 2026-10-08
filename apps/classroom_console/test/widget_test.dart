import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/main.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/student_file_drop.dart';

class LiveAgent extends Agent {
  LiveAgent() : super([]);
  @override
  bool get connected => true;
  String? observed;
  @override
  Future<void> openObserve(Map<String, dynamic> row) async {
    observed = row["id"] as String;
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final windowCalls = <String>[];
  setUp(() {
    windowCalls.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('classroom/permissions'),
          (call) async => call.method == 'status'
              ? {
                  'screen': true,
                  'helperScreen': true,
                  'appPath': '/fixture/Console.app',
                  'helperPath': '/fixture/Agent.app',
                }
              : true,
        );
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(const MethodChannel('window_manager'), (
          call,
        ) async {
          windowCalls.add(call.method);
          return null;
        });
  });
  testWidgets('Repeated grid/list clicks and selection', (tester) async {
    tester.view.physicalSize = const Size(1100, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final a = LiveAgent();
    a.state = {
      'phase': 'idle',
      'students': [
        {
          'id': 'one',
          'name': 'Lab PC 01',
          'ip': '10.0.55.11',
          'online': true,
          'monitoring': 'ready',
        },
      ],
    };
    await tester.pumpWidget(ClassroomApp(agent: a));
    if (Platform.isMacOS) {
      await tester.pumpAndSettle();
      expect(find.text('Set up macOS permissions'), findsOneWidget);
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
    }
    expect(find.text('Lab PC 01'), findsOneWidget);
    expect(find.byType(StudentFileDrop), findsOneWidget);
    expect(find.text('Stop sharing'), findsNothing);
    expect(find.textContaining('Share selected'), findsNothing);
    expect(find.text('Ready to teach'), findsNothing);
    for (var i = 0; i < 5; i++) {
      await tester.tap(find.byIcon(Icons.view_list_outlined));
      await tester.pumpAndSettle();
      expect(find.byType(StudentFileDrop), findsOneWidget);
      await tester.tap(find.byIcon(Icons.format_list_bulleted));
      await tester.pumpAndSettle();
      expect(find.byType(AspectRatio), findsNothing);
      expect(find.byType(StudentFileDrop), findsOneWidget);
      await tester.tap(find.byIcon(Icons.grid_view));
      await tester.pumpAndSettle();
    }
    expect(find.byType(Checkbox), findsNothing);
    await tester.tap(find.text('Lab PC 01'));
    await tester.pumpAndSettle();
    expect(a.observed, 'one');
    expect(find.byType(Dialog), findsNothing);
    await tester.tap(find.byTooltip('Select computers for presentation'));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Checkbox));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(find.text('Share selected (1)'), findsOneWidget);
    expect(tester.takeException(), isNull);
    tester.view.physicalSize = const Size(740, 520);
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(Icons.format_list_bulleted));
    await tester.pumpAndSettle();
    expect(find.text('Share selected (1)'), findsOneWidget);
    expect(tester.takeException(), isNull);
    a.state['phase'] = 'starting';
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(windowCalls.where((m) => m == 'minimize'), isEmpty);
    a.state['phase'] = 'sharing';
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(find.text('Stop sharing'), findsOneWidget);
    expect(windowCalls.where((m) => m == 'minimize').length, 1);
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(windowCalls.where((m) => m == 'minimize').length, 1);
    expect(find.text('Share class'), findsNothing);
    expect(find.textContaining('Share selected'), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    a.dispose();
  });
  testWidgets('Capture permission error opens native settings from banner', (
    tester,
  ) async {
    final calls = <String>[];
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('classroom/permissions'),
          (call) async {
            calls.add(call.method);
            return true;
          },
        );
    final a = LiveAgent()
      ..state = {'phase': 'idle', 'students': <Map<String, dynamic>>[]}
      ..error = 'ScreenCaptureKit TCC declined';
    await tester.pumpWidget(ClassroomApp(agent: a));
    if (Platform.isMacOS) {
      await tester.pumpAndSettle();
      expect(find.text('Set up macOS permissions'), findsOneWidget);
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
    }
    await tester.pumpAndSettle();
    if (Platform.isMacOS) {
      expect(find.text('Open Settings'), findsOneWidget);
      await tester.tap(find.text('Open Settings'));
      await tester.pumpAndSettle();
      expect(find.text('Set up macOS permissions'), findsOneWidget);
      await tester.tap(find.text('Request / Open Settings'));
      await tester.pumpAndSettle();
      expect(calls, contains('openScreenRecordingSettings'));
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
    } else {
      expect(find.text('Open Settings'), findsNothing);
      expect(find.text('ScreenCaptureKit TCC declined'), findsOneWidget);
      expect(calls, isEmpty);
    }
    await tester.pumpWidget(const SizedBox());
    a.dispose();
  });
  testWidgets(
    'Grid size slider changes columns and Auto restores responsive layout',
    (tester) async {
      tester.view.physicalSize = const Size(1100, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final a = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': List.generate(
            6,
            (i) => {
              'id': '$i',
              'name': 'PC $i',
              'ip': '10.0.55.${i + 1}',
              'online': true,
              'monitoring': 'ready',
            },
          ),
        };
      await tester.pumpWidget(ClassroomApp(agent: a));
      if (Platform.isMacOS) {
        await tester.pumpAndSettle();
        expect(find.text('Set up macOS permissions'), findsOneWidget);
        await tester.tap(find.text('Continue'));
        await tester.pumpAndSettle();
      }
      await tester.pumpAndSettle();
      int columns() =>
          ((tester.widget<GridView>(find.byType(GridView)).gridDelegate)
                  as SliverGridDelegateWithFixedCrossAxisCount)
              .crossAxisCount;
      final before = columns();
      expect(find.byType(Slider), findsOneWidget);
      await tester.drag(find.byType(Slider), const Offset(80, 0));
      await tester.pumpAndSettle();
      expect(columns(), lessThan(before));
      expect(
        tester.widget<FilterChip>(find.byType(FilterChip)).selected,
        false,
      );
      await tester.tap(find.text('Auto'));
      await tester.pumpAndSettle();
      expect(columns(), before);
      tester.view.physicalSize = const Size(740, 520);
      await tester.pumpAndSettle();
      expect(columns(), 2);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      a.dispose();
    },
  );
}
