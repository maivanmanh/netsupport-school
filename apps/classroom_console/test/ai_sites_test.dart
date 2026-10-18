import 'package:classroom_console/agent.dart';
import 'package:classroom_console/ai_sites_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Plays the Teacher: keeps the list and the switch the way the real one does.
class AiAgent extends Agent {
  AiAgent(List<String> sites) : super([]) {
    state = {
      'tools': {
        'ai': {'sites': sites, 'blocking': false, 'default_count': 3},
      },
    };
  }
  final calls = <Map<String, dynamic>>[];
  static const defaults = ['https://chatgpt.com', 'claude.ai', 'poe.com'];
  @override
  bool get connected => true;

  Map get ai => (state['tools'] as Map)['ai'] as Map;

  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    calls.add({'action': action, 'input': input});
    switch (action) {
      case 'ai_sites_set':
        ai['sites'] = List<String>.from(input!['sites'] as List);
      case 'ai_sites_reset':
        ai['sites'] = List<String>.from(defaults);
      case 'ai_block_set':
        ai['blocking'] = input!['enabled'];
    }
    notifyListeners();
    return {'ok': true};
  }
}

Future<void> open(WidgetTester tester, AiAgent agent) async {
  tester.view.physicalSize = const Size(1100, 900);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
  await tester.pumpWidget(
    MaterialApp(
      home: Builder(
        builder: (context) => TextButton(
          onPressed: () => showAiSites(context, agent),
          child: const Text('Open'),
        ),
      ),
    ),
  );
  await tester.tap(find.text('Open'));
  await tester.pumpAndSettle();
}

void main() {
  test('hosts, paths and bad entries', () {
    expect(
      aiHostOf('https://www.Adobe.com/products/firefly.html'),
      'www.adobe.com',
    );
    expect(aiHostOf('chatgpt.com'), 'chatgpt.com');
    expect(aiHostOf('http://localhost:3000/x'), 'localhost');
    expect(aiEntryHasPath('https://github.com/features/copilot'), isTrue);
    expect(aiEntryHasPath('https://chatgpt.com'), isFalse);
    expect(aiEntryHasPath('https://chatgpt.com/'), isFalse);
    expect(aiEntryProblem('', const []), isNotNull);
    expect(aiEntryProblem('not a site', const []), isNotNull);
    expect(
      aiEntryProblem('chatgpt', const []),
      isNotNull,
      reason: 'needs a dot',
    );
    expect(aiEntryProblem('chatgpt.com', const []), isNull);
    expect(
      aiEntryProblem('https://CHATGPT.com/x', ['chatgpt.com']),
      contains('đã có'),
    );
    expect(
      aiEntryProblem('chatgpt.com', ['chatgpt.com'], except: 'chatgpt.com'),
      isNull,
    );
  });

  group('AI websites dialog', () {
    testWidgets('lists the sites and marks those that block a whole site', (
      tester,
    ) async {
      final agent = AiAgent([
        'https://chatgpt.com',
        'https://github.com/features/copilot',
      ]);
      await open(tester, agent);
      expect(find.text('chatgpt.com'), findsOneWidget);
      expect(find.text('github.com'), findsOneWidget);
      expect(find.text('cả trang'), findsOneWidget);
      expect(find.text('Hiện 2'), findsNothing);
      agent.dispose();
    });

    testWidgets('add, edit and delete send the whole new list', (tester) async {
      final agent = AiAgent(['chatgpt.com', 'claude.ai']);
      await open(tester, agent);

      await tester.enterText(find.byKey(const ValueKey('ai-add')), 'poe.com');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ai-add-button')));
      await tester.pumpAndSettle();
      expect(agent.calls.last['action'], 'ai_sites_set');
      expect(agent.calls.last['input']['sites'], [
        'chatgpt.com',
        'claude.ai',
        'poe.com',
      ]);
      expect(find.text('poe.com'), findsOneWidget);

      // A duplicate is refused without bothering the Teacher agent.
      final before = agent.calls.length;
      await tester.enterText(
        find.byKey(const ValueKey('ai-add')),
        'https://claude.ai/new',
      );
      await tester.tap(find.byKey(const ValueKey('ai-add-button')));
      await tester.pumpAndSettle();
      expect(agent.calls.length, before);
      expect(find.textContaining('đã có trong danh sách'), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('ai-edit-claude.ai')));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField).last, 'claude.com');
      await tester.tap(find.text('Lưu'));
      await tester.pumpAndSettle();
      expect(agent.calls.last['input']['sites'], [
        'chatgpt.com',
        'claude.com',
        'poe.com',
      ]);

      await tester.tap(find.byKey(const ValueKey('ai-delete-chatgpt.com')));
      await tester.pumpAndSettle();
      expect(agent.calls.last['input']['sites'], ['claude.com', 'poe.com']);
      expect(find.text('chatgpt.com'), findsNothing);
      agent.dispose();
    });

    testWidgets(
      'typing offers the sites already listed, so nothing is typed twice',
      (tester) async {
        final agent = AiAgent(['https://chatgpt.com', 'claude.ai', 'poe.com']);
        await open(tester, agent);
        await tester.enterText(find.byKey(const ValueKey('ai-add')), 'cha');
        await tester.pumpAndSettle();
        expect(find.byKey(const ValueKey('ai-suggestions')), findsOneWidget);
        expect(
          find.byKey(const ValueKey('ai-suggest-https://chatgpt.com')),
          findsOneWidget,
        );
        expect(find.text('Đã có trong danh sách'), findsOneWidget);
        expect(find.byKey(const ValueKey('ai-suggest-poe.com')), findsNothing);

        // Choosing one adds nothing: it is shown in the list instead.
        final before = agent.calls.length;
        await tester.tap(
          find.byKey(const ValueKey('ai-suggest-https://chatgpt.com')),
        );
        await tester.pumpAndSettle();
        expect(agent.calls.length, before);
        expect(
          tester
              .widget<TextField>(find.byKey(const ValueKey('ai-add')))
              .controller!
              .text,
          '',
        );
        expect(
          tester
              .widget<TextField>(find.byKey(const ValueKey('ai-search')))
              .controller!
              .text,
          'chatgpt.com',
        );
        expect(
          find.textContaining('đã có sẵn trong danh sách'),
          findsOneWidget,
        );
        agent.dispose();
      },
    );

    testWidgets('a repeat is flagged while typing and cannot be added', (
      tester,
    ) async {
      final agent = AiAgent(['chatgpt.com']);
      await open(tester, agent);
      final button = find.byKey(const ValueKey('ai-add-button'));
      expect(
        tester.widget<FilledButton>(button).onPressed,
        isNull,
        reason: 'nothing typed',
      );
      await tester.enterText(
        find.byKey(const ValueKey('ai-add')),
        'https://www.ChatGPT.com',
      );
      await tester.pumpAndSettle();
      // www. makes it a different host: allowed.
      expect(tester.widget<FilledButton>(button).onPressed, isNotNull);
      await tester.enterText(
        find.byKey(const ValueKey('ai-add')),
        'CHATGPT.com',
      );
      await tester.pumpAndSettle();
      expect(
        find.textContaining('“chatgpt.com” đã có trong danh sách'),
        findsOneWidget,
      );
      expect(tester.widget<FilledButton>(button).onPressed, isNull);
      await tester.enterText(
        find.byKey(const ValueKey('ai-add')),
        'gemini.google.com',
      );
      await tester.pumpAndSettle();
      expect(tester.widget<FilledButton>(button).onPressed, isNotNull);
      agent.dispose();
    });

    testWidgets('search narrows the list', (tester) async {
      final agent = AiAgent(['chatgpt.com', 'claude.ai', 'poe.com']);
      await open(tester, agent);
      await tester.enterText(find.byKey(const ValueKey('ai-search')), 'cla');
      await tester.pumpAndSettle();
      expect(find.text('claude.ai'), findsOneWidget);
      expect(find.text('poe.com'), findsNothing);
      await tester.enterText(find.byKey(const ValueKey('ai-search')), 'zzz');
      await tester.pumpAndSettle();
      expect(find.text('Không có website nào khớp'), findsOneWidget);
      agent.dispose();
    });

    testWidgets(
      'reset brings the original list back after everything was deleted',
      (tester) async {
        final agent = AiAgent(['claude.ai']);
        await open(tester, agent);
        await tester.tap(find.byKey(const ValueKey('ai-delete-claude.ai')));
        await tester.pumpAndSettle();
        expect(find.text('Danh sách đang trống'), findsOneWidget);
        await tester.tap(find.byKey(const ValueKey('ai-reset')));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Khôi phục'));
        await tester.pumpAndSettle();
        expect(agent.calls.last['action'], 'ai_sites_reset');
        expect(find.text('chatgpt.com'), findsOneWidget);
        expect(find.text('poe.com'), findsOneWidget);
        agent.dispose();
      },
    );

    testWidgets('the switch blocks and unblocks, and needs a non-empty list', (
      tester,
    ) async {
      final agent = AiAgent(['chatgpt.com']);
      await open(tester, agent);
      await tester.tap(find.byKey(const ValueKey('ai-block-switch')));
      await tester.pumpAndSettle();
      expect(agent.calls.last, {
        'action': 'ai_block_set',
        'input': {'enabled': true},
      });
      expect(find.textContaining('Đang chặn 1 website'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('ai-block-switch')));
      await tester.pumpAndSettle();
      expect(agent.calls.last['input'], {'enabled': false});
      expect(find.textContaining('Đang tắt'), findsOneWidget);

      // With no sites the switch cannot be turned on.
      await tester.tap(find.byKey(const ValueKey('ai-delete-chatgpt.com')));
      await tester.pumpAndSettle();
      final tile = tester.widget<SwitchListTile>(
        find.byKey(const ValueKey('ai-block-switch')),
      );
      expect(tile.onChanged, isNull);
      agent.dispose();
    });

    testWidgets('a refusal from the Teacher is shown with its reason', (
      tester,
    ) async {
      final agent = _Refusing(['chatgpt.com']);
      await open(tester, agent);
      await tester.enterText(find.byKey(const ValueKey('ai-add')), 'poe.com');
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('ai-add-button')));
      await tester.pumpAndSettle();
      expect(find.textContaining('Tối đa 200 website'), findsOneWidget);
      agent.dispose();
    });
  });
}

class _Refusing extends AiAgent {
  _Refusing(super.sites);
  @override
  Future<Map<String, dynamic>> request(
    String action, {
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
    Duration timeout = const Duration(seconds: 5),
  }) async {
    return {
      'ok': false,
      'error': 'rejected',
      'result': {'error_detail': 'Tối đa 200 website'},
    };
  }
}
