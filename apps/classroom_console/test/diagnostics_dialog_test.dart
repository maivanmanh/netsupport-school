import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:classroom_console/agent.dart';
import 'package:classroom_console/diagnostics_dialog.dart';

class DiagnosticAgent extends Agent {
  DiagnosticAgent() : super([]);
  bool reject = false;
  List<String> requested = [];
  String? lastCommand;
  @override
  Future<void> commandChecked(
    String action,
    Iterable<String> selected,
    Map<String, dynamic> input,
  ) async {
    if (reject) throw StateError('fixture disconnected');
    lastCommand = action;
    requested = selected.toList();
  }

  @override
  Future<void> command(
    String action, [
    Iterable<String> selected = const [],
    Map<String, dynamic>? input,
  ]) async {
    lastCommand = action;
    if (action == 'cancel_diagnostics') {
      state['diagnostics'] = {'phase': 'cancelled'};
      notifyListeners();
    }
  }
}

Future<void> open(WidgetTester tester, DiagnosticAgent agent) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Builder(
          builder: (context) => TextButton(
            onPressed: () =>
                collectDiagnostics(context, agent, ['online', 'offline']),
            child: const Text('Collect'),
          ),
        ),
      ),
    ),
  );
  await tester.tap(find.text('Collect'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('queued collection explains Stop and can be cancelled', (
    tester,
  ) async {
    final agent = DiagnosticAgent()
      ..state = {
        'diagnostics': {'phase': 'waiting_for_stop'},
      };
    await open(tester, agent);
    expect(agent.lastCommand, 'collect_diagnostics');
    expect(agent.requested, ['online', 'offline']);
    expect(find.textContaining('sau khi bạn dừng chiếu'), findsOneWidget);
    await tester.tap(find.text('Hủy thu thập'));
    await tester.pumpAndSettle();
    expect(agent.lastCommand, 'cancel_diagnostics');
    expect(find.text('Đã hủy thu thập.'), findsOneWidget);
    expect(find.text('Hủy thu thập'), findsNothing);
    await tester.tap(find.byTooltip('Đóng'));
    await tester.pumpAndSettle();
    agent.dispose();
  });

  testWidgets('completed ZIP distinguishes missing computers', (tester) async {
    final agent = DiagnosticAgent()
      ..state = {
        'diagnostics': {
          'phase': 'complete',
          'collected': 1,
          'total': 2,
          'missing': 1,
          'path': '/fixture/diagnostics.zip',
          'students': [
            {
              'name': 'PC 02',
              'ip': '10.0.55.12',
              'status': 'not_collected',
              'reason': 'offline',
            },
          ],
        },
      };
    await open(tester, agent);
    expect(find.textContaining('1/2 học viên'), findsOneWidget);
    expect(find.textContaining('/fixture/diagnostics.zip'), findsOneWidget);
    expect(find.text('PC 02 (10.0.55.12): offline'), findsOneWidget);
    expect(find.text('Hiện tệp ZIP'), findsOneWidget);
    expect(find.text('Hủy thu thập'), findsNothing);
    await tester.tap(find.byTooltip('Đóng'));
    await tester.pumpAndSettle();
    agent.dispose();
  });

  testWidgets('rejected request explains failure without opening dialog', (
    tester,
  ) async {
    final agent = DiagnosticAgent()..reject = true;
    await open(tester, agent);
    expect(find.byType(AlertDialog), findsNothing);
    expect(find.textContaining('fixture disconnected'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    agent.dispose();
  });
}
