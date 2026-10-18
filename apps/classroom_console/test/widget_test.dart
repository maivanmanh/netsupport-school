import 'package:classroom_console/live_activity.dart';
import 'dart:io';
import 'package:flutter/gestures.dart';
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
  setUpAll(() => BlinkingRecordDot.animated = false);
  TestWidgetsFlutterBinding.ensureInitialized();
  testWidgets(
    'offline toggle hides remembered PCs and restores them without removing roster',
    (tester) async {
      tester.view.physicalSize = const Size(1100, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': [
            {
              'id': 'on',
              'name': 'Online PC',
              'ip': '10.0.68.117',
              'online': true,
            },
            {
              'id': 'off',
              'name': 'Offline PC',
              'ip': '10.0.68.118',
              'online': false,
            },
          ],
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      expect(find.text('Offline PC'), findsOneWidget);
      expect(find.text('Tự động'), findsNothing);
      expect(
        find.byTooltip('Tự động sắp xếp kích thước ô máy'),
        findsOneWidget,
      );
      await tester.tap(find.byTooltip('Ẩn máy offline'));
      await tester.pumpAndSettle();
      expect(find.text('Offline PC'), findsNothing);
      expect(find.text('Online PC'), findsOneWidget);
      expect(agent.students, hasLength(2));
      expect(agent.preferences['show_offline'], false);
      await tester.tap(find.byTooltip('Hiện máy offline'));
      await tester.pumpAndSettle();
      expect(find.text('Offline PC'), findsOneWidget);
      expect(agent.preferences['show_offline'], true);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
  testWidgets(
    'receiving FPS and speed (bytes) appear in grid, list and compact views',
    (tester) async {
      tester.view.physicalSize = const Size(1300, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = LiveAgent()
        ..state = {
          'phase': 'sharing',
          'students': [
            {
              'id': 'one',
              'name': 'Lab PC 01',
              'ip': '10.0.55.11',
              'online': true,
              'media': {
                'state': 'receiving',
                'render_fps': 23.6,
                'receive_mbps': 24.0,
              },
            },
          ],
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      // Grid: numbers only.
      expect(find.text('24 FPS · 2.9 MB/s'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.view_list_outlined));
      await tester.pumpAndSettle();
      expect(find.textContaining('23.6 FPS · 2.9 MB/s'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.format_list_bulleted));
      await tester.pumpAndSettle();
      expect(find.textContaining('23.6 FPS · 2.9 MB/s'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
  testWidgets(
    'evidence shows as a camera count on the computer and announces new pictures',
    (tester) async {
      tester.view.physicalSize = const Size(1300, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      Map<String, dynamic> shot(String id) => {
        'id': id,
        'student': 'one',
        'label': 'chatgpt.com',
        'address': 'chatgpt.com/c/1',
        'title': 'x',
        'reason': 'conversation',
        'taken_ms': 1,
        'bytes': 10,
        'path': '/nowhere/$id.jpg',
      };
      final agent = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': [
            {
              'id': 'one',
              'name': 'Lab PC 01',
              'ip': '10.0.55.11',
              'online': true,
            },
          ],
          'tools': {
            'evidence': {
              'items': [shot('1'), shot('2')],
            },
          },
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      // What was already there is shown but not announced.
      expect(find.byKey(const ValueKey('evidence-badge-one')), findsOneWidget);
      expect(find.text('2'), findsWidgets);
      expect(find.textContaining('đã lưu ảnh minh chứng'), findsNothing);
      // A new picture arrives while the console is open.
      ((agent.state['tools'] as Map)['evidence'] as Map)['items'] = [
        shot('3'),
        shot('1'),
        shot('2'),
      ];
      agent.notifyListeners();
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));
      expect(
        find.textContaining(
          'Lab PC 01 đang dùng chatgpt.com — đã lưu ảnh minh chứng',
        ),
        findsOneWidget,
      );
      expect(find.text('Xem'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
  testWidgets(
    'a Student on an AI site is highlighted in every view, browser in front or not',
    (tester) async {
      tester.view.physicalSize = const Size(1300, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': [
            {
              'id': 'ai',
              'name': 'Lab PC 01',
              'ip': '10.0.55.11',
              'online': true,
            },
            {
              'id': 'ok',
              'name': 'Lab PC 02',
              'ip': '10.0.55.12',
              'online': true,
            },
          ],
          'tools': {
            'activity_monitoring': true,
            'activity_live': {
              // ChatGPT in a background Chrome while VS Code is in front.
              'ai': {
                'apps': [
                  {'app': 'Code.exe', 'title': 'main.rs'},
                  {'app': 'chrome.exe', 'title': 'ChatGPT'},
                ],
                'url': 'https://chatgpt.com/c/1',
                'url_in_front': false,
                'ai': ['chatgpt.com'],
              },
              'ok': {
                'apps': [
                  {'app': 'Code.exe', 'title': 'main.rs'},
                ],
                'url': 'https://docs.rs',
                'url_in_front': false,
              },
            },
          },
        };
      agent.state['students'] = [
        {'id': 'ai', 'name': 'Lab PC 01', 'ip': '10.0.55.11', 'online': true},
        {'id': 'ok', 'name': 'Lab PC 02', 'ip': '10.0.55.12', 'online': true},
      ];
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      void expectHighlight() {
        expect(find.text('AI · chatgpt.com'), findsOneWidget);
        expect(
          find.byTooltip('Đang mở website AI: chatgpt.com'),
          findsOneWidget,
        );
        // The other Student shows its page but no warning.
        expect(find.textContaining('docs.rs'), findsOneWidget);
      }

      expectHighlight();
      await tester.tap(find.byIcon(Icons.view_list_outlined));
      await tester.pumpAndSettle();
      expectHighlight();
      await tester.tap(find.byIcon(Icons.format_list_bulleted));
      await tester.pumpAndSettle();
      expectHighlight();
      // Without activity monitoring nothing is flagged.
      (agent.state['tools'] as Map)['activity_monitoring'] = false;
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.textContaining('AI ·'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
  testWidgets(
    'live activity, blocks and recording time appear in every roster view',
    (tester) async {
      tester.view.physicalSize = const Size(1300, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': [
            {
              'id': 'one',
              'name': 'Lab PC 01',
              'ip': '10.0.55.11',
              'online': true,
            },
          ],
          'tools': {
            'activity_monitoring': true,
            'activity_live': {
              'one': {
                'apps': [
                  {'app': 'chrome.exe', 'title': 'Docs'},
                  {'app': 'Code.exe', 'title': 'main.rs'},
                ],
                'url': 'https://www.example.org/page',
                'tab_title': 'Example',
              },
            },
            'apps_blocked': ['one'],
            'internet': {'one': 'block_all'},
            'recordings': [
              {
                'id': 'r',
                'targets': [
                  {
                    'id': 'one',
                    'report': {'phase': 'recording', 'elapsed_seconds': 125},
                  },
                ],
              },
            ],
          },
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      Future<void> expectAll() async {
        expect(find.textContaining('example.org'), findsOneWidget);
        expect(find.byTooltip('Chrome — Docs'), findsOneWidget);
        expect(find.byTooltip('VS Code — main.rs'), findsOneWidget);
        expect(
          find.byTooltip('Đang chặn ứng dụng trên máy này'),
          findsOneWidget,
        );
        expect(find.byTooltip('Internet: Chặn tất cả'), findsOneWidget);
        expect(
          find.byTooltip('Đang ghi màn hình • đã ghi 02:05'),
          findsOneWidget,
        );
      }

      await expectAll(); // grid
      await tester.tap(find.byIcon(Icons.view_list_outlined));
      await tester.pumpAndSettle();
      await expectAll();
      await tester.tap(find.byIcon(Icons.format_list_bulleted));
      await tester.pumpAndSettle();
      await expectAll();
      // Monitoring off: the live strip goes away, the blocks stay.
      (agent.state['tools'] as Map)['activity_monitoring'] = false;
      agent.notifyListeners();
      await tester.pumpAndSettle();
      expect(find.textContaining('example.org'), findsNothing);
      expect(find.byTooltip('Đang chặn ứng dụng trên máy này'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
  testWidgets(
    'recording status appears in every roster view and context menu',
    (tester) async {
      tester.view.physicalSize = const Size(1100, 800);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final agent = LiveAgent()
        ..state = {
          'phase': 'idle',
          'students': [
            {
              'id': 'one',
              'name': 'Lab PC 01',
              'ip': '10.0.55.11',
              'online': true,
            },
          ],
          'tools': {
            'recordings': [
              {
                'id': 'recording-session',
                'targets': [
                  {
                    'id': 'one',
                    'report': {'phase': 'recording'},
                  },
                ],
              },
            ],
          },
        };
      await tester.pumpWidget(ClassroomApp(agent: agent));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Đang ghi màn hình'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.view_list_outlined));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Đang ghi màn hình'), findsOneWidget);
      await tester.tap(find.byIcon(Icons.format_list_bulleted));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Đang ghi màn hình'), findsOneWidget);
      await tester.tap(find.text('Lab PC 01'), buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();
      expect(find.text('Dừng ghi màn hình'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
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
        tester
            .widget<IconButton>(find.byKey(const ValueKey('grid-auto-toggle')))
            .isSelected,
        false,
      );
      await tester.tap(find.byKey(const ValueKey('grid-auto-toggle')));
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
