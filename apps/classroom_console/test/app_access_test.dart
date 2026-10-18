import 'package:classroom_console/agent.dart';
import 'package:classroom_console/app_access.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_store.dart';

class RecordingAppsAgent extends AppsAgent {
  RecordingAppsAgent(super.store);
  final sent = <Map<String, dynamic>>[];
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    if (action == 'tool_app_policy') {
      sent.add(Map<String, dynamic>.from(input!['policy'] as Map));
      return {'ok': true, 'job_id': 'job'};
    }
    return super.request(
      action,
      selected: selected,
      input: input,
      timeout: timeout,
    );
  }
}

class AppsAgent extends Agent with FakeStoreAgent {
  AppsAgent(this.store) : super([]);
  @override
  final FakeStore store;
  @override
  bool get connected => true;
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async => action == 'app_policy_state'
      ? {
          'ok': true,
          'result': {
            'rule': {
              'scope': null,
              'value': {
                'enabled': true,
                'mode': 'block',
                'ids': ['aaaaaaaaaaaa', 'bbbbbbbbbbbb'],
              },
            },
          },
        }
      : {'ok': false};
}

void main() {
  testWidgets(
    'reopening restores both applied checkboxes with stale preferences',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1024, 768));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final store = FakeStore()
        ..apply('inventory', {
          'cache': {
            'apps': [
              {
                'id': 'aaaaaaaaaaaa',
                'name': 'Calculator',
                'executables': 1,
                'students': ['pc'],
              },
              {
                'id': 'bbbbbbbbbbbb',
                'name': 'Paint',
                'executables': 1,
                'students': ['pc'],
              },
            ],
          },
        });
      for (var opening = 0; opening < 2; opening++) {
        final agent = AppsAgent(store);
        await tester.pumpWidget(
          MaterialApp(
            home: AppsWindow(
              key: ValueKey(opening),
              agent: agent,
              studentIds: const ['pc'],
              audience: 'class',
              classWide: true,
            ),
          ),
        );
        for (var pending = 0; pending < 4; pending++) {
          await tester.pump();
          await tester.runAsync(() async {
            await Future<void>.delayed(const Duration(milliseconds: 100));
          });
        }
        await tester.pump();
        await tester.runAsync(() async {
          await (tester.state(find.byType(AppsWindow)) as dynamic).loadCache();
        });
        await tester.pump();
        expect(find.text('Calculator'), findsOneWidget);
        expect(find.text('Paint'), findsOneWidget);
        expect(
          tester
              .widgetList<Checkbox>(find.byType(Checkbox))
              .map((c) => c.value),
          [true, true],
        );
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
        agent.dispose();
      }
    },
  );

  testWidgets(
    'Apply lights only while the selection differs from what is applied',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1024, 768));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final store = FakeStore()
        ..apply('inventory', {
          'cache': {
            'apps': [
              {
                'id': 'aaaaaaaaaaaa',
                'name': 'Calculator',
                'executables': 1,
                'students': ['pc'],
              },
              {
                'id': 'bbbbbbbbbbbb',
                'name': 'Paint',
                'executables': 1,
                'students': ['pc'],
              },
            ],
          },
        });
      final agent = AppsAgent(store)
        ..state = {
          'students': [
            {'id': 'pc', 'online': true},
          ],
          'tools': {
            'capabilities': {
              'pc': {'apps': 1},
            },
          },
        };
      await tester.pumpWidget(
        MaterialApp(
          home: AppsWindow(
            agent: agent,
            studentIds: const ['pc'],
            audience: 'class',
            classWide: true,
          ),
        ),
      );
      for (var pending = 0; pending < 4; pending++) {
        await tester.pump();
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 100)),
        );
      }
      await tester.runAsync(
        () async =>
            (tester.state(find.byType(AppsWindow)) as dynamic).loadCache(),
      );
      await tester.pump();
      ButtonStyleButton apply() => tester.widget<ButtonStyleButton>(
        find.ancestor(
          of: find.text('Áp dụng'),
          matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
        ),
      );
      // Restrictions already match the selection: nothing to apply.
      expect(apply().onPressed, isNull);
      await tester.tap(find.byType(Checkbox).last);
      await tester.pump();
      expect(apply().onPressed, isNotNull);
      // Undoing the edit returns to the applied state.
      await tester.tap(find.byType(Checkbox).last);
      await tester.pump();
      expect(apply().onPressed, isNull);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );

  testWidgets(
    'path rules: add, validate, edit, delete and apply independently',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final store = FakeStore()
        ..apply('inventory', {
          'cache': {
            'apps': [
              {
                'id': 'aaaaaaaaaaaa',
                'name': 'Calculator',
                'executables': 1,
                'students': ['pc'],
              },
            ],
          },
        });
      final agent = RecordingAppsAgent(store)
        ..state = {
          'students': [
            {'id': 'pc', 'online': true},
          ],
          'tools': {
            'capabilities': {
              'pc': {'apps': 1},
            },
            'jobs': [
              {
                'id': 'job',
                'targets': [
                  {'status': 'complete'},
                ],
              },
            ],
          },
        };
      await tester.pumpWidget(
        MaterialApp(
          home: AppsWindow(
            agent: agent,
            studentIds: const ['pc'],
            audience: 'class',
            classWide: true,
          ),
        ),
      );
      for (var pending = 0; pending < 4; pending++) {
        await tester.pump();
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 100)),
        );
      }
      ButtonStyleButton button(String label) =>
          tester.widget<ButtonStyleButton>(
            find.ancestor(
              of: find.text(label),
              matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
            ),
          );
      await tester.tap(find.text('Theo đường dẫn'));
      await tester.pump();
      expect(find.text('Chưa có đường dẫn nào bị chặn'), findsOneWidget);
      expect(button('Áp dụng').onPressed, isNull);
      // An invalid entry cannot be added and says why.
      await tester.tap(find.text('Mới…'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, 'games');
      await tester.pump();
      expect(find.textContaining('đường dẫn đầy đủ'), findsOneWidget);
      expect(button('Thêm').onPressed, isNull);
      await tester.enterText(
        find.byType(TextField).last,
        r'C:\Windows\System32',
      );
      await tester.pump();
      expect(find.textContaining('hệ thống'), findsOneWidget);
      // Whole operating-system folders (not just System32) are refused too,
      // whichever way they are written; a subfolder of Program Files is fine.
      for (final system in [
        r'C:\Program Files',
        r'c:\program files (x86)\\',
        r'C:\ProgramData',
        r'C:\Users',
        r'%ProgramFiles%',
        r'%WINDIR%\Temp',
        r'%SystemDrive%',
      ]) {
        await tester.enterText(find.byType(TextField).last, system);
        await tester.pump();
        expect(find.textContaining('hệ thống'), findsOneWidget, reason: system);
        expect(button('Thêm').onPressed, isNull, reason: system);
      }
      await tester.enterText(
        find.byType(TextField).last,
        r'C:\Program Files\Game',
      );
      await tester.pump();
      expect(find.textContaining('hệ thống'), findsNothing);
      expect(button('Thêm').onPressed, isNotNull);
      await tester.enterText(
        find.byType(TextField).last,
        'C:\\Games\n%USERPROFILE%\\Downloads\\setup.exe',
      );
      await tester.pump();
      expect(button('Thêm').onPressed, isNotNull);
      await tester.tap(find.text('Thêm'));
      await tester.pumpAndSettle();
      expect(find.text(r'C:\Games'), findsOneWidget);
      expect(find.text('thư mục'), findsOneWidget);
      expect(find.text('tệp'), findsOneWidget);
      expect(button('Áp dụng').onPressed, isNotNull);
      // Deleting the pending rows returns to the applied (empty) state.
      await tester.tap(find.text(r'C:\Games'));
      await tester.pump();
      await tester.tap(find.text('Xóa'));
      await tester.pump();
      await tester.tap(find.textContaining('setup.exe'));
      await tester.pump();
      await tester.tap(find.text('Xóa'));
      await tester.pump();
      expect(button('Áp dụng').onPressed, isNull);
      // Add one again and apply: the program list goes along as it was applied.
      await tester.tap(find.text('Mới…'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, r'D:\Tools\run.exe');
      await tester.pump();
      await tester.tap(find.text('Thêm'));
      await tester.pumpAndSettle();
      // Editing keeps Save off until the text actually differs.
      await tester.tap(find.text('Sửa…'));
      await tester.pumpAndSettle();
      expect(button('Lưu').onPressed, isNull);
      await tester.enterText(
        find.byType(TextField).last,
        r'D:\Tools\other.exe',
      );
      await tester.pump();
      expect(button('Lưu').onPressed, isNotNull);
      await tester.tap(find.text('Lưu'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Áp dụng'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Áp dụng').last);
      await tester.pumpAndSettle();
      expect(agent.sent, hasLength(1));
      expect(agent.sent.single['paths'], [r'D:\Tools\other.exe']);
      expect(agent.sent.single['mode'], 'block');
      expect(agent.sent.single['enabled'], true);
      expect(agent.sent.single['ids'], ['aaaaaaaaaaaa', 'bbbbbbbbbbbb']);
      expect(button('Áp dụng').onPressed, isNull);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
}
