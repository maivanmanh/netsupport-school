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
      expect(find.text('Set up macOS permissions'), findsNothing);
    }
    expect(find.text('Lab PC 01'), findsOneWidget);
    expect(find.byType(StudentFileDrop), findsOneWidget);
    expect(find.byTooltip('Dừng chiếu'), findsNothing);
    expect(find.textContaining('Chiếu cho máy đã chọn'), findsNothing);
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
    await tester.tap(find.byTooltip('Chọn nhiều máy để thao tác'));
    await tester.pumpAndSettle();
    await tester.tap(find.byType(Checkbox));
    await tester.pump(const Duration(milliseconds: 400));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Chiếu cho máy đã chọn (1)'), findsOneWidget);
    expect(tester.takeException(), isNull);
    tester.view.physicalSize = const Size(740, 520);
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(Icons.format_list_bulleted));
    await tester.pumpAndSettle();
    expect(find.byTooltip('Chiếu cho máy đã chọn (1)'), findsOneWidget);
    expect(tester.takeException(), isNull);
    a.state['phase'] = 'starting';
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(windowCalls.where((m) => m == 'minimize'), isEmpty);
    a.state['phase'] = 'sharing';
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(find.byTooltip('Dừng chiếu'), findsOneWidget);
    expect(
      find.byTooltip('Điều khiển & chiếu màn hình học viên'),
      findsNothing,
    );
    expect(windowCalls.where((m) => m == 'minimize').length, 1);
    a.notifyListeners();
    await tester.pumpAndSettle();
    expect(windowCalls.where((m) => m == 'minimize').length, 1);
    expect(find.byTooltip('Chiếu màn hình giảng viên'), findsNothing);
    expect(find.textContaining('Chiếu cho máy đã chọn'), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
    a.dispose();
  });
  testWidgets('Missing Screen Recording is requested on Share, not warned', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1280, 800);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final calls = <String>[];
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('classroom/permissions'),
          (call) async {
            calls.add(call.method);
            if (call.method == 'screenRecordingGranted') return false;
            if (call.method == 'status') {
              return {
                'screen': true,
                'helperScreen': true,
                'appPath': '/fixture/Console.app',
                'helperPath': '/fixture/Agent.app',
              };
            }
            return true;
          },
        );
    final a = LiveAgent()
      ..state = {
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
      }
      ..error = 'ScreenCaptureKit TCC declined';
    await tester.pumpWidget(ClassroomApp(agent: a));
    await tester.pumpAndSettle();
    // The permission error is no longer a standing banner on any platform.
    expect(find.text('ScreenCaptureKit TCC declined'), findsNothing);
    expect(find.byType(MaterialBanner), findsNothing);
    if (Platform.isMacOS) {
      await tester.tap(find.byTooltip('Chiếu màn hình giảng viên'));
      await tester.pumpAndSettle();
      expect(calls, contains('openScreenRecordingSettings'));
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
        expect(find.text('Set up macOS permissions'), findsNothing);
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
      await tester.tap(find.text('Tự động'));
      await tester.pumpAndSettle();
      expect(columns(), before);
      tester.view.physicalSize = const Size(740, 520);
      await tester.pumpAndSettle();
      expect(columns(), greaterThanOrEqualTo(2));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      a.dispose();
    },
  );
}
