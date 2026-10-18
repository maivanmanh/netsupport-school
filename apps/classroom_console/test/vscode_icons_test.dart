import 'package:classroom_console/agent.dart';
import 'package:classroom_console/vscode_access.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'support/fake_store.dart';

// A valid 1x1 PNG.
const _png =
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

class ExtensionsAgent extends Agent with FakeStoreAgent {
  ExtensionsAgent(this.store) : super([]);
  @override
  final FakeStore store;
  final iconRequests = <String, List>{};
  final _jobs = <String, Map<String, dynamic>>{};
  @override
  bool get connected => true;
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    if (action == 'vscode_list') {
      final wanted = (input?['icons_for'] as List?) ?? const [];
      final student = '${input?['student']}';
      if (wanted.isNotEmpty) iconRequests[student] = wanted;
      final id = 'job${_jobs.length}';
      _jobs[id] = {
        'extensions': [
          for (final ext in ['ms-python.python', 'esbenp.prettier-vscode'])
            {
              'id': ext,
              'name': ext.split('.').last,
              'description': '',
              'publisher': ext.split('.').first,
              if (wanted.contains(ext) && ext.startsWith('ms-'))
                'icon_png_base64': _png,
            },
        ],
        'notes': <String>[],
      };
      return {'ok': true, 'job_id': id};
    }
    if (action == 'fs_result') {
      return {'ok': true, 'result': _jobs['${input?['id']}']};
    }
    return {'ok': true};
  }
}

void main() {
  testWidgets('extension icons are requested once, shown, and cached', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1100, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final store = FakeStore();
    final agent = ExtensionsAgent(store)
      ..state = {
        'students': [
          {'id': 'a', 'online': true},
          {'id': 'b', 'online': true},
        ],
      };
    await tester.pumpWidget(
      MaterialApp(
        home: VsCodeWindow(
          agent: agent,
          audience: 'class',
          studentIds: const ['a', 'b'],
        ),
      ),
    );
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(milliseconds: 300));
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 60)),
      );
    }
    expect(find.text('python'), findsOneWidget);
    expect(find.text('prettier-vscode'), findsOneWidget);
    expect(find.byType(Image), findsOneWidget);
    expect(find.byIcon(Icons.extension), findsOneWidget);
    // Each id was asked of exactly one Student.
    final asked = agent.iconRequests.values.expand((v) => v).toList()..sort();
    expect(asked, ['esbenp.prettier-vscode', 'ms-python.python']);
    // The icon is kept for the next time the window opens.
    final saved = await store.list('vscode_icons');
    expect((saved['ms-python.python'] as Map)['png'], _png);
    await tester.pumpWidget(const SizedBox());
    agent.dispose();
  });

  testWidgets(
    'unresolved %placeholders% from an older Student show the extension name',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final agent = PlaceholderAgent(FakeStore())
        ..state = {
          'students': [
            {'id': 'a', 'online': true},
          ],
        };
      await tester.pumpWidget(
        MaterialApp(
          home: VsCodeWindow(
            agent: agent,
            audience: 'class',
            studentIds: const ['a'],
          ),
        ),
      );
      for (var i = 0; i < 8; i++) {
        await tester.pump(const Duration(milliseconds: 300));
        await tester.runAsync(
          () => Future<void>.delayed(const Duration(milliseconds: 60)),
        );
      }
      expect(find.textContaining('%'), findsNothing);
      expect(find.text('remote-containers'), findsOneWidget);
      expect(find.text('Dev Containers'), findsOneWidget);
      await tester.pumpWidget(const SizedBox());
      agent.dispose();
    },
  );
}

class PlaceholderAgent extends ExtensionsAgent {
  PlaceholderAgent(super.store);
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    if (action == 'vscode_list') return {'ok': true, 'job_id': 'j'};
    if (action == 'fs_result') {
      return {
        'ok': true,
        'result': {
          'extensions': [
            {
              'id': 'ms-vscode-remote.remote-containers',
              'name': '%displayName%',
              'description': '%description%',
              'publisher': 'ms-vscode-remote',
            },
            {
              'id': 'ms-python.python',
              'name': 'Dev Containers',
              'description': '',
              'publisher': 'ms-python',
            },
          ],
          'notes': <String>[],
        },
      };
    }
    return {'ok': true};
  }
}
